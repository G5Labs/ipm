# Installing web applications

A module can declare an IRIS web application and ship the files it serves. Reinstall, upgrade and uninstall use the application name in the manifest.

## Definitions

WebApplication is the current manifest element. CSPApplication is the older element. DeployPath is the destination for files that the older processor copies.

## Application ownership and changes on upgrade

<a id="module.webapp"></a>
[D · module.webapp]

A `<WebApplication>` resource creates the named web application during install and deletes it during uninstall. IPM does not record who created an application, so it treats any application with that name as the module's own.

- If an application with that name already exists, for example one an administrator made, install changes only the properties the manifest declares and keeps every other property. Uninstalling the module then deletes that application.
- Reinstalling resets declared properties that someone edited by hand, and keeps edits to properties the manifest does not declare.
- A second module that declares a web application name another installed module already lists is refused when its manifest is read, so two modules cannot share one application.
- `NameSpace` defaults to the namespace being installed into. Whether IPM should take over or delete applications it did not create is an [open question](../../Verus_verification/open_questions.md#module.webapp.ownership).
- Attribute values are expanded like other resource attributes, so `{$mgrdir}` becomes the manager directory. Loading again writes only the attributes still declared, so a removed attribute keeps its previous value. `compile -only` does not touch the application, while Activate and Configure update it.

- Working assumption: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.

<a id="module.webapp.upgrade"></a>
[D · module.webapp.upgrade]

Upgrading to a version whose manifest no longer declares a web application leaves that application in place. A later uninstall deletes only the applications the installed version declares, so the old application is left behind for good (C:D26).

- Working assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.

## Validation and failed installation

<a id="module.webapp.validation"></a>
[D · module.webapp.validation]

An attribute that is not a web application property, such as a misspelled one, is passed through without an error and has no effect. The processor has a check for unknown attributes, but the lifecycle's Validate step never calls it (C:D23).

<a id="module.webapp.provider-status"></a>
[D · module.webapp.provider-status]

Updating an existing web app calls Modify only when a declared value differs from its old value. An absent old property reads as empty, so a declared empty value for it is not a change.

- If no value differs, the phase succeeds without calling Modify. When a value differs, a returned Modify failure becomes a failed phase status; it is not discarded. A failed Get or Create also fails the phase.
- Switching into `%SYS` for the call restores the caller's namespace on return or error. Provider failure alone does not promise that the provider made no partial changes.
- This refines iteration 5's predicate: its abstract missing-key-is-change branch is not a complete account of empty strings.

<a id="module.webapp.failure"></a>
[D · module.webapp.failure]

The application is created before the module's later steps run. If the install then fails inside its transaction, the rollback also removes the application. With `-DNoTransaction=1`, the application and the module record both stay after the failure.

## Packaging an application

<a id="module.webapp.package"></a>
[D · module.webapp.package]

Web-app resources are configuration and are marked handled during export, so their paths are not exported as database documents. Publishing preserves their declarations in the package.

After uninstall removes the app, installing the published package recreates it from those declarations. A `{$cspdir}` Path is evaluated against the installation's CSP root, so it survives the round trip and follows a different CSP root on another installation.

This guarantee assumes successful package/registry transport and app creation. It does not promise compensation for a partial provider failure.

## Legacy CSP applications

<a id="module.csp.directory"></a>
[D · module.csp.directory]

For a legacy CSPApplication with no Directory and no DispatchClass, IPM uses the normalized module root plus resource name as its file directory. With a DispatchClass alone it first sets the validation sentinel `<irrelevant>`, then clears that sentinel during phase processing, so the final Directory is empty.

An explicit Directory is normalized and takes precedence over dispatch inference. The sentinel is an intermediate value, not a directory that readers should expect after the call.

<a id="module.csp.lifecycle"></a>
[D · module.csp.lifecycle]

A deprecated `<CSPApplication>` creates or updates its web application during Activate and deletes it during uninstall. Each load prints a deprecation warning and copies the source folder into `DeployPath` when one is given.

- Updating an existing application writes every property the processor knows, so properties the manifest does not declare, such as a description or timeout an administrator set, return to the processor defaults. `WebApplication` keeps them (defect C:D61).
- Uninstall also deletes the files of the application's directory. As built, without `DeployPath` that directory is the module's own source folder, so uninstalling deletes the module's source files (defect C:D60).

## Metadata

### module.webapp

- design-verified

- uncertain
- agent
- user-facing-behavior
- Expands: [Processors](resources.g5.md#module.processor).
- Children: [failure](#module.webapp.failure), [upgrade](#module.webapp.upgrade), [validation](#module.webapp.validation), [Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?](../../Verus_verification/open_questions.md#module.webapp.ownership).

Direct decision: `ResourceProcessor/WebApplication.cls::{OnBeforePhase,CreateOrUpdateWebApp,DeleteWebApp,CopyAttributes,OnGetUniqueName}`. Readers and callers: `Storage/Module.cls::ExecutePhases` (processor `OnBeforePhase` for Configure, Activate, Unconfigure and Clean), `Storage/Module.cls` duplicate-resource check at manifest import (`:1568`), `%SYS` `Security.Applications::{Exists,Get,Modify,Create,Delete}`. CHANGELOG 0.9.0 `#562`. Verus [webapp.rs](../../Verus_verification/consolidated/iter5/webapp.rs) `{merged,install_app,modify_called,refused_at_import,lemma_adoption_keeps_undeclared,webapp_neighbors}`; controls `adoption-overwrites-undeclared`, `existing-app-not-adopted`, `modify-always-called` rejected. Native W1 (create), W2/W3 (administrator app adopted, then deleted), W4 (second module refused), W5 (manual edits) in a fresh container; replayed as Verus assertions. Iteration 5 discovery.


Iteration61: [webappphases.rs](../../Verus_verification/consolidated/iter61/webappphases.rs) `apply`, `load`, `neighbors` ([handoff](../../uplift/consolidated/iter61/HANDOFF.md)).

Open questions: [Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?](../../Verus_verification/open_questions.md#module.webapp.ownership)

State machines: [webapp · iter5](../../Verus_verification/models/iter5-webapp/manifest.json), [webappphases · iter61](../../Verus_verification/models/iter61-webappphases/manifest.json)

### module.webapp.upgrade

- uncertain
- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Readers: `Storage/Module.cls::ExecutePhases` and `Lifecycle/Base.cls::%Clean` act only on the installed manifest's resources; no step compares the old and new manifests. Verus `upgrade_drops_declaration`; control `upgrade-removes-app` rejected. Native W10 (load 2.0.0 without the app, then uninstall: app still present). The `update` command from a registry was not run; this used `load` of a higher version.

Open questions: [Should a higher-version load remove a web application omitted by the new manifest?](../../Verus_verification/open_questions.md#module.webapp.upgrade.policy)

### module.webapp.validation

- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Dead check: `ResourceProcessor/WebApplication.cls::OnPhase` (Validate branch, `DoPropertiesExist`). Missing caller: `Lifecycle/Base.cls::%Validate` returns OK without dispatching; the generic `ResourceProcessor/Abstract.cls::OnBeforePhase("Validate")` only runs `%ValidateObject`. CHANGELOG 0.10.11 `#806` quotes the refusal message as once user-visible. Verus `validate_dispatches`, `refused_at_validate`; control `validate-dispatches` rejected. Native W9 (`NoSuchProperty="1"`: load OK, app created).

### module.webapp.provider-status

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.webapp](#module.webapp).

Sources and readers: `ResourceProcessor/WebApplication.cls::CreateOrUpdateWebApp` → `OnBeforePhase` → `Storage/Module.cls::ExecutePhases`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `differs`, `app_status`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.webapp.failure

- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Direct decision: transaction scope of the load (`Utils/Module.cls` load path) over `%SYS` writes from `CreateOrUpdateWebApp`. Verus `failed_after_activate`; control `transaction-keeps-app` rejected. Native W8 (transaction: app and module gone) versus W8b (`-DNoTransaction=1`: both kept), using a fixture invoke that fails after Activate. Contrast: [Must package failure undo instance configuration changes?](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy) are not undone by rollback. Developer-mode commit points were not exercised.

### module.webapp.package

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [module.webapp](#module.webapp).

Sources and readers: `ResourceProcessor/WebApplication.cls::OnExportItem` → `Lifecycle/Base.cls::{ExportSingleModule,%Publish}`, `Lifecycle/Module.cls::%Package`, registry payload/manifest readers → `WebApplication.cls::CreateOrUpdateWebApp`; final reader `Security.Applications.Get`. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/webapps.rs): `package_app`, `evaluate_path`, `uninstall`, `reinstall`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [webapps · iter8](../../Verus_verification/models/iter8-webapps/manifest.json)

### module.csp.directory

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.processor](resources.g5.md#module.processor).

Sources and readers: `ResourceProcessor/CSPApplication.cls::OnBeforePhase` → `OnPhaseInternal`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `csp_directory`, `csp_after_before`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.


Findings: [Uninstalling a CSPApplication without DeployPath deletes the module's source folder](../../Verus_verification/bugs_issues.md#c-d60)

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.csp.lifecycle

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](resources.g5.md#module.processor).

Sources: `ResourceProcessor/CSPApplication.cls::OnPhaseInternal`, `::CreateOrUpdateCSPApp`, `::DeleteCSPApp`. Model: [cspapp.rs](../../Verus_verification/consolidated/iter57/cspapp.rs) `load`, `uninstall`, `no_deploy_path_uninstall_deletes_source`, `takeover_resets_undeclared` ([handoff](../../uplift/consolidated/iter57/HANDOFF.md)). Deleting an application IPM did not create is the open [Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?](../../Verus_verification/open_questions.md#module.webapp.ownership).

Findings: [Uninstalling a CSPApplication without DeployPath deletes the module's source folder](../../Verus_verification/bugs_issues.md#c-d60)

Findings: [A CSPApplication load resets an existing application's undeclared properties](../../Verus_verification/bugs_issues.md#c-d61)

State machines: [cspapp · iter57](../../Verus_verification/models/iter57-cspapp/manifest.json)
