# Iteration 57: deprecated CSPApplication processor from load to uninstall

Convergence probe (strongest remaining consequential candidate). Product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter57/cspapp.rs), [cases and controls](../../../Verus_verification/consolidated/iter57/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owners: [module.webapp](../../../ontology/concepts/modules.g5.md#module.webapp) for the WebApplication contrast, and [module.csp.directory](../../../ontology/concepts/modules.g5.md#module.csp.directory) for the default directory. Not sealed; the main thread integrates.

## Predictions and observations

Owner predictions were recorded before the processor body was read; source predictions were added after.

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| a1 `load m57a` (DeployPath `/tmp/iter57/deployA/`, not existing yet) | application `/ipmcont57a` created with that path and unauthenticated access; `index.html` copied (v1); deprecation warning printed | created, files copied | created; no copy, because the directory did not exist (**refuted**) |
| a2 edit source to v2, `load m57a` again | deploy copy is v2 | updated | updated |
| a3 `uninstall m57a` | application gone, deploy files gone; source folder kept | application deleted; files unspecified | deletePortal purges deploy files |
| a4 `load m57a -dev` after uninstall | recreated and copied | created | created |
| b1 `load m57b` (no DeployPath) | application path is the module's own `web/` folder | | path = module root + resource name |
| b2 `uninstall m57b` | application gone and **the module's `web/` source folder and its file are deleted** | application deleted | purge of the source folder |
| c0 administrator creates `/ipmcont57c` (description, timeout 1234, password auth, own path) | present | | |
| c1 `load m57c` | path set to deployC, description cleared, timeout reset, password auth replaced by unauthenticated | like WebApplication: undeclared properties kept | every attribute written, undeclared reset |
| c2 `uninstall m57c` | the administrator's application is deleted | deleted (module.webapp.ownership as built) | deleted |

## Evidence

- Formal: `cspapp.rs` 5 verified, 0 errors. Controls `purge-skips-source-folder`, `takeover-keeps-undeclared`, `no-copy-on-load` and `uninstall-keeps-deploy-files` each rejected. Run 1 stopped because `uninstall-keeps-deploy-files` survived before the `deploy_path_uninstall_purges_deploy` lemma existed; it is kept in `attempts/proofs-run1-deploy-control-survived`.
- Native: [receipt](native/20261116T182314549001Z/observations.json), 174 fields, all gated, image `sha256:7da4480b…`, network none. Replay: 18 fields in Verus ([receipt](replay/receipt.json)). Flips of `b2.sourceB`, `c1.appc.descadmin` and `a2.deployA` each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter57/run.py` (11 s).

## Discrepancies (new in this search)

1. **High (proposed):** uninstalling a module whose `<CSPApplication>` has no `DeployPath` deletes the module's own source folder. The default Directory is the module root plus the resource name (owned by `module.csp.directory`), and `DeleteCSPApp` purges the files of that Directory through `deletePortal`. For a module loaded from a working copy this erases the user's files. E2E reproduced (b1, b2); passing neighbour: with `DeployPath` only the deployed copy is purged and the source survives (a3).
2. **Low (proposed):** loading a CSPApplication over an existing application resets every property the manifest does not declare (description, timeout, authentication), unlike `WebApplication`, which keeps undeclared properties. E2E reproduced (c0, c1).

## Reach candidates

Newly reached: `CSPApplication.cls::OnAfterPhase`, `::CreateOrUpdateCSPApp`, `::DeleteCSPApp`. Strengthened: `::OnBeforePhase`, `::OnPhaseInternal`, `Utils/File.cls::CopyDir`, `::IsProtectedDirectory`, `Lifecycle/Base.cls::%Clean`. Not claimed: `%OnNew` (warning observed, not modelled), `MakeDirectoryWriteable`, `WarnIfInInstallDirectory`, `ReplaceMatchRoles`, `ActivateDelegated`, `OnBeforeArtifact` (package scope), `OnGetUniqueName`, `GetSourceControlInfo`, `OnItemRelativePath`.

## Limits

MatchRoles, delegated authentication, DispatchClass, protected deploy directories (covered by the product FileCopy test) and the CSP gateway cache are not exercised. Security.Applications, deletePortal and the copy/chmod commands are providers. No ObjectScript body is verified directly.

## Proposed owner text

New statement in `.g5/ontology/concepts/modules.g5.md`, after `module.csp.directory`:

```
<a id="module.csp.lifecycle"></a>
[D · module.csp.lifecycle]

A deprecated `<CSPApplication>` creates or updates its web application during Activate and deletes it during uninstall. Each load prints a deprecation warning and copies the source folder into `DeployPath` when one is given.

- Updating an existing application writes every property the processor knows, so properties the manifest does not declare, such as a description or timeout an administrator set, return to the processor defaults. `WebApplication` keeps them (defect C:D-next-2).
- Uninstall also deletes the files of the application's directory. As built, without `DeployPath` that directory is the module's own source folder, so uninstalling deletes the module's source files (defect C:D-next-1).
```

Metadata: `agent`, `user-facing-behavior`, Expands [Resource processors](#module.processor). Sources: `ResourceProcessor/CSPApplication.cls::OnPhaseInternal`, `::CreateOrUpdateCSPApp`, `::DeleteCSPApp`. Model: [cspapp.rs](../../Verus_verification/consolidated/iter57/cspapp.rs) `load`, `uninstall`, `no_deploy_path_uninstall_deletes_source`, `takeover_resets_undeclared` ([handoff](../../uplift/consolidated/iter57/HANDOFF.md)). Add Findings lines for both entries; `module.csp.directory` metadata should also link C:D-next-1.

Proposed C register entries:

- **High · Decision: Open — "Uninstalling a CSPApplication without DeployPath deletes the module's source folder".** Found by: source review of `CSPApplication.cls::DeleteCSPApp` against the default Directory, iteration57 model `no_deploy_path_uninstall_deletes_source`, control `purge-skips-source-folder`. Reproduced by: E2E reproduced, receipt `iter57/native/20261116T182314549001Z`: after `load m57b`, `uninstall m57b` removes `/tmp/iter57/m57b/web/` and `index.html` (b2). Passing neighbour: with `DeployPath`, uninstall purges only the deployed copy and the source survives (a3). Repair, unchecked: never purge when Directory is the module's resource folder (or only purge a DeployPath).
- **Low · Decision: Open — "A CSPApplication load resets an existing application's undeclared properties".** Found by: source review of `CreateOrUpdateCSPApp` (every ATTRIBUTES property written), model `takeover_resets_undeclared`, control `takeover-keeps-undeclared`. Reproduced by: E2E reproduced, same receipt: an administrator's `/ipmcont57c` loses its description, timeout 1234 and password authentication after `load m57c` (c0, c1). Passing neighbour: WebApplication keeps undeclared properties (module.webapp, iteration5). Repair, unchecked: write only declared attributes when the application exists.
