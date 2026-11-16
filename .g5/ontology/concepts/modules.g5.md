# Modules and the manifest

A module is what IPM installs. This page defines what a module's manifest can declare. How IPM acts on those declarations is owned by the [lifecycle](lifecycle.g5.md), [loading](../features/load.g5.md) and the other feature pages.

## Definitions

- **Manifest**: the `module.xml` file that describes a module.
- **Resource**: one item a module installs, such as a class, routine, include file, global, file or web application.
- **Unique resource name**: the key used to prevent two stored resource references from claiming the same item; a processor may supply a key different from the manifest name.
- **Resource processor**: the code that installs, compiles and removes one kind of resource.

## Statements

<a id="module"></a>
[I · module]

A module is a named, versioned package of code and data that IPM installs into a namespace. Its manifest, `module.xml`, describes its resources, dependencies, settings and install steps.

<a id="module.identity"></a>
[I · module.identity]

A manifest gives the module a `Name`, a [semantic version](versions.g5.md#version) and descriptive fields.

- The manifest is a `<Document>` named `<Name>.ZPM`.
- An optional `ExternalName` can be used in commands in place of the name. It must be unique and must not match another module's name.

<a id="module.resource"></a>
[I · module.resource]

A module lists its resources, and IPM picks a resource processor for each one from its type or element.

- Resource attributes include `Deploy`, `Directory`, `Preload` and `Scope`.
- Processors with special behavior are listed under [resource processors](#module.processor).

<a id="module.resource.identity"></a>
[D · module.resource.identity]

When saving a module, IPM refuses a new resource reference if its nonempty unique name is already stored, even for the same module. Reusing the stored reference passes this check; the other save checks still apply.

- The check uses the resource's stored identity and unique name, not just the module name. It runs before a deployed-project installer can be called.
- A refused save can leave changed object fields that were not written to the row. Read the database separately when deciding what survived.

<a id="module.resource.scope"></a>
[I · module.resource.scope]

A resource with `Scope="test"` stays out of ordinary installs. It loads and compiles in the Test phase, and packaging leaves it out unless tests are included.

<a id="module.dependency"></a>
[I · module.dependency]

A `<ModuleReference>` names a dependency and the versions it accepts, either exact or a [range](versions.g5.md#version.range). An omitted version means any version. How IPM picks versions is owned by [dependency resolution](../features/dependencies.g5.md#deps).

<a id="module.parameter"></a>
[I · module.parameter]

`<Default>` and `<Defaults><Parameter>` give values the module receives at load and install. A `-D<name>=<value>` on the command line overrides them.

- Dotted names become nested parameters.
- Placeholders in values are filled in, repeatedly if needed.
- Defaults can also set the [transaction flags](transactions-and-dev-mode.g5.md#transaction.flag-sources).

<a id="module.parameter.expressions"></a>
[I · module.parameter.expressions]

Attribute values in a manifest, such as file targets, invoke arguments and the after-install message, can contain expressions that IPM fills in at install time. A value without a `{` is used as written.

- `${name}` or `{$name}` takes a parameter's value. Names match regardless of case. The built-in `root`, `packagename`, `version`, `verbose` and `ipmDir` take precedence over module defaults with the same name.
- The same forms also give instance values such as `namespace`, `namespaceLower` and `mgrDir`.
- `$$$name` takes the value of an IRIS macro, or nothing if the macro is undefined.
- `{name}` gives the module name, and `#{expression}` gives the value of an ObjectScript expression, or nothing if it fails.

- As built, a value without `{` returns unchanged before parameter lookup, macro expansion or expression compilation. With `{`, lookup and nested placeholders run, followed by macro/expression evaluation.

- A replacement containing `$` may raise; a dotted parameter name can match another name because it becomes regex syntax. A cyclic reference exhausts the loop before the declared cycle error can run (C:D3–D5).

- These failures contradict literal-name/value intent. [C:Q2](../../Verus_verification/open_questions.md#module.parameter.expressions.brace-policy) asks whether brace-free macros should expand; it does not leave the observed gate undecided.

- Working assumption: keep the observed brace gate until a maintainer chooses otherwise.

<a id="module.parameter.literal-values"></a>
[I · module.parameter.literal-values]

A parameter value is inserted exactly as written. Characters such as `$` and `\` in a value must not change the result or cause an error.

<a id="module.parameter.cycles"></a>
[I · module.parameter.cycles]

If parameters refer to each other in a loop, IPM should report an error that names the circular reference instead of installing with an unfilled value.

- This requirement is recovered from the named error and guard in the product, rather than an owner decision. As built, the guard is unreachable: `a=${b}`, `b=${a}` returns unresolved `${a}` without that error. A resolving neighbor, `a=${b}`, `b=ok`, returns `ok`.
- Using an unresolved value later may raise a regular-expression error. This defect does not satisfy the circular-reference requirement. [Iter 2 native reader](../../uplift/consolidated/iter2/native/receipt.json) and [retained Verus counterexample](../../Verus_verification/consolidated/verus/attr_evaluator.rs) preserve both outcomes.

<a id="module.requirement"></a>
[I · module.requirement]

`<SystemRequirements>` lists conditions that must hold before the module is installed. A failed condition [refuses the load](../features/load.g5.md#load.refusal.requirements).

- `Version`: the IRIS versions the module supports.
- `Interoperability`: the namespace must have interoperability enabled.
- `Health`: the instance must be HealthShare, HealthConnect or IRIS for Health.
- `IPMVersion` and `PythonVersion`: the IPM and Python versions it needs. A mapped namespace has a [local IPM-version reader gap](namespaces.g5.md#namespace.version.mapped-reader).
- `SYSNamespace`: the module must be installed in `%SYS`.

<a id="module.requirement.values"></a>
[D · module.requirement.values]

An omitted Health requirement accepts either kind of instance; explicit false excludes Health products, while true requires one. Health true also requires an interoperability-enabled namespace, even with `Interoperability="disabled"`.

- `Interoperability="enabled"` requires an enabled namespace. As built, `disabled` does not require it to be disabled. The tests accept that declaration without an exclusion check.
- `SYSNamespace="true"` requires the exact `%SYS` namespace. False or omission adds no namespace restriction.
- The aggregate appends returned errors from its checks in order rather than stopping after the first returned error. A thrown exception may stop later checks. Version parsing, runtime detection and the known SemVer exceptions remain separate dependencies.

<a id="module.invoke"></a>
[I · module.invoke]

`<Invoke>` runs a class method during a lifecycle phase, by default after Configure. By default IPM checks a declared status return; a returned error then fails the phase.

- Explicit `CheckStatus="0"` discards the return, even for a status method. `CheckStatus="1"` checks the return regardless of its declared type. A thrown exception still fails the invocation. These deprecated overrides are part of the pinned contract.

- A nonempty `CustomPhase` replaces `Phase`. The phase and Before/After side must both match; it does not add a second call in the ordinary phase. See [custom phases](lifecycle.g5.md#lifecycle.custom).

<a id="module.processor"></a>
[I · module.processor]

Resource processors with special behavior include:

- `FileCopy`, which copies files or folders ([file copy](#module.processor.file-copy)).
- `WebApplication`, which sets up [web applications](#module.webapp). `CSPApplication` is deprecated.
- `CPF`, which merges configuration into the instance.
- `SystemSetting`, [Artifactory downloads](../features/artifact-download.g5.md#artifact), unit test and localization processors.

Processor defaults and processing order are [owned here](#module.processor.defaults).

<a id="module.processor.defaults"></a>
[D · module.processor.defaults]

A processor copies explicit attributes before applying matching defaults in declaration order. An explicit attribute, including an empty value, protects that property from defaults; without one, a later matching default can replace an earlier default.

- Each default tests its conditions against the processor's current values, including changes from earlier defaults. It must name the same processor class, using its full name or the built-in short form; inheritance alone is not a match.
- Supplying parameters again reevaluates attributes and defaults. It does not turn a default into an explicit attribute.
- Named document resources run first, sorted by name without the extension. Equal names keep their declaration order. Path-style and other resources follow in declaration order. The formal contract checks pair ordering and default precedence; a complete sorting refinement and all comparison operators remain open.

<a id="module.webapp"></a>
[D · module.webapp]

A `<WebApplication>` resource creates the named web application during install and deletes it during uninstall. IPM does not record who created an application, so it treats any application with that name as the module's own.

- If an application with that name already exists, for example one an administrator made, install changes only the properties the manifest declares and keeps every other property. Uninstalling the module then deletes that application.
- Reinstalling resets declared properties that someone edited by hand, and keeps edits to properties the manifest does not declare.
- A second module that declares a web application name another installed module already lists is refused when its manifest is read, so two modules cannot share one application.
- `NameSpace` defaults to the namespace being installed into. Whether IPM should take over or delete applications it did not create is an [open question](../../Verus_verification/open_questions.md#module.webapp.ownership).

- Working assumption: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.

<a id="module.webapp.failure"></a>
[D · module.webapp.failure]

The application is created before the module's later steps run. If the install then fails inside its transaction, the rollback also removes the application. With `-DNoTransaction=1`, the application and the module record both stay after the failure.

<a id="module.webapp.upgrade"></a>
[D · module.webapp.upgrade]

Upgrading to a version whose manifest no longer declares a web application leaves that application in place. A later uninstall deletes only the applications the installed version declares, so the old application is left behind for good (C:D26).

- Working assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.

<a id="module.webapp.validation"></a>
[D · module.webapp.validation]

An attribute that is not a web application property, such as a misspelled one, is passed through without an error and has no effect. The processor has a check for unknown attributes, but the lifecycle's Validate step never calls it (C:D23).

<a id="module.processor.file-copy"></a>
[I · module.processor.file-copy]

`FileCopy` copies files at install and removes them at uninstall.

- It never deletes IRIS or IPM system folders, or any folder that contains one.
- It refuses to copy when the source and target are the same folder or one contains the other.
- `KeepOnUninstall` leaves the copied files in place on `uninstall` and `clean`, for files other modules may share.
- `Overlay=0` requests full directory replacement, including removal of unrelated old entries. The pinned Unix path has a [replacement discrepancy](#module.processor.file-copy.unix-overlay).

- Immediate copies run before Activate; `Defer` runs them after successful Activate. A failing hook between them leaves an immediate target but prevents the deferred copy.

- Database rollback does not restore either filesystem state. With `KeepOnUninstall`, Clean preserves the target; other directory targets are removed subject to the [current protection check](#module.processor.file-copy.protection).

- Directory overlay rules protect system directories. These cases were checked through both the processor and an actual failing load; no general filesystem compensation is proved.
- Direct `DoCopy` with a missing source returns an error and creates no copied target file. A successful copy remains after database rollback while a journaled database sentinel disappears; rollback does not turn a failed copy into a successful one. Creating the destination directory and partial operating-system copy errors are separate effects.

- Working assumption: Protect configured roots and their ancestors and an exact target containing an IRIS.DAT marker.

<a id="module.placement"></a>
[I · module.placement]

A module controls where its code and data live.

- `GlobalScope` makes it available in all namespaces without per-namespace mappings.
- `NoMapping` turns off the automatic mappings for its resources.
- `<Mappings>` adds explicit mappings.
- `SourcesRoot` names the folder that holds its code.
- Expressions such as `${ipmDir}` stand for system folders.

<a id="module.deployed"></a>
[I · module.deployed]

Deployment ships eligible code without its source. An explicit resource `Deploy` value takes precedence over the lifecycle default, unless a named deployment parameter exists on that class: then only parameter value `1` enables deployment, even when the resource says true.

- The normal Module lifecycle defaults to false; DeployedModule defaults to true. The class parameter override applies only to an existing class with that parameter.
- The default comes from the lifecycle class's `DEPLOYBYDEFAULT` parameter and is used only when the resource has no `Deploy` attribute. A custom lifecycle inherits its parent's default unless it sets its own.
- Built-in compiled-only deployment applies to classes and MAC/INT routines. Include files and other types keep their source form unless their own processor handles deployment. IPM picks the package built for the current IRIS version and platform and installs code into each resource's mapped database.
- Loading a deployed project has separate [eligibility and retry rules](#module.deployed.project-load).

<a id="module.deployed.project-load"></a>
[D · module.deployed.project-load]

A deployed resource loads from the module's Studio project only when its type is deployable or its name is a path, the project file exists, and the resource is neither generated nor preloaded. Otherwise the ordinary source path remains in use.

- IPM sets the object's Deployed flag before attempting its save. If save fails, the database row can remain false while the object says true, and the installer is not called. After save succeeds, a corrupt-project install error can leave the stored flag true; neither flag proves installation.
- A process-local marker is set only after successful project installation. Other resources using that path then skip the installer. Failure leaves no success marker, so a later attempt may retry. Provider writes before failure are not assumed atomic. A fresh duplicate resource reference can cause the earlier save to fail; [reusing its stored identity](#module.resource.identity) is a different input.
- Packaging's project check has a different purpose: it removes missing project items and can return success after doing so. Check success alone does not establish that every requested item remains in the project.

<a id="module.python"></a>
[I · module.python]

A module can depend on Python packages.

- IPM installs `requirements.txt` with pip in the Reload phase, and `<PythonWheel>` files in Initialize.
- `-bypass-py-deps` skips this, and `-extra-pip-flags` adds pip flags.
- Python 3.9 or later is required. The `PipCaller` and `UseStandalonePip` settings pick which pip runs.

- Working assumption: Clean preserves the current shared Python installation.

<a id="module.env"></a>
[I · module.env]

`-env` takes `;`-separated JSON files of settings for the module, merges them and replaces `${VAR}` with environment values. The module reads them with `%IPM.General.EnvironmentConfig:GetArg`, and a missing file is an error.

- Files merge in order. A later value replaces an earlier one, and nested objects merge key by key.
- Every command that runs module phases starts from an empty configuration and loads only its own `-env` files. A later command in the same shell session does not see an earlier command's files.
- While a configuration is loaded, IPM compiles the module's classes in one process, so code generated at compile time can read it.
- What a failing file should leave behind, and how arrays merge, is [open](../../Verus_verification/open_questions.md#module.env.failure).
- An environment value is inserted literally. As built, a `$` or `\` in the value changes or breaks the result. This is a known defect.

- Object elements in arrays merge recursively at the same array index: `[{"x":1}]` followed by `[{"y":2}]` keeps both keys. Primitive arrays merge by position: `[1,2]` then `[3]` yields `[3,2]`.

- Both direct merge and the actual file loader have receipts. A missing or malformed later file clears earlier values and raises; each `LoadEnvironmentConfig` call clears first.

- Process-local config reaches a same-process compile, while a fresh worker process starts empty. These are observed implementation rules; C:Q6 below keeps the desired failure/merge policy open.

- Working assumption: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.

<a id="module.processor.cpf-effects"></a>
[D · module.processor.cpf-effects]

A CPF processor runs a child command that changes instance configuration. Its exit status and the final configuration must be read separately from database rollback.

- In the disposable IRIS provider, a valid role-creation merge returned success and the role survived rollback, while a journaled sentinel disappeared. An invalid-section neighbor returned error with child exit 7 and created no role.
- These observations cover one configuration action and one refusal. Other actions, partial failures and settings that need restart remain external premises; exit zero alone does not prove their effect.

- Working assumption: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.

<a id="module.global.roundtrip"></a>
[D · module.global.roundtrip]

A global resource is exported and read back at `<SourcesRoot>/<Directory>/<global name>.xml`; Directory defaults to `gbl/`. Packaging a global with SourcesRoot="src", clearing it and loading that package restores its stored values.

- The name drops `.GBL`; its special characters are translated by the processor's filename settings. A preload resource has `preload/` before SourcesRoot.
- This round-trip contract covers ordinary, non-generated, non-preload globals with ordinary names. Mapping, preservation on uninstall and custom filename translation retain source evidence and open formal gaps.

<a id="module.processor.default-sweep"></a>
[D · module.processor.default-sweep]

After successful individual replacements, a processor default's custom-parameter substitution makes one pass in parameter-name order. A replacement that introduces a later parameter can be filled in during that pass; one that introduces an already-visited parameter remains unfilled here.

This is the `EvaluateAttribute` helper used by processor attributes, not the repeated placeholder-resolution step for module defaults. The scoped rule covers ordinary parameter names and successfully decoded replacement values. A raw `$` in a replacement can fail before the next pass, as in the [existing literal-value defect](#module.parameter.literal-values). The native raw forward/backward examples both fail at that provider. Successful introduction of another placeholder by ordinary replacement values has not been established; the abstract sweep alone does not close that case.

<a id="module.csp.directory"></a>
[D · module.csp.directory]

For a legacy CSPApplication with no Directory and no DispatchClass, IPM uses the normalized module root plus resource name as its file directory. With a DispatchClass alone it first sets the validation sentinel `<irrelevant>`, then clears that sentinel during phase processing, so the final Directory is empty.

An explicit Directory is normalized and takes precedence over dispatch inference. The sentinel is an intermediate value, not a directory that readers should expect after the call.

<a id="module.webapp.provider-status"></a>
[D · module.webapp.provider-status]

Updating an existing web app calls Modify only when a declared value differs from its old value. An absent old property reads as empty, so a declared empty value for it is not a change.

- If no value differs, the phase succeeds without calling Modify. When a value differs, a returned Modify failure becomes a failed phase status; it is not discarded. A failed Get or Create also fails the phase.
- Switching into `%SYS` for the call restores the caller's namespace on return or error. Provider failure alone does not promise that the provider made no partial changes.
- This refines iteration 5's predicate: its abstract missing-key-is-change branch is not a complete account of empty strings.

<a id="module.list.delimiter"></a>
[D · module.list.delimiter]

An XML option-list value is split at its configured ODBC delimiter. An omitted or comma delimiter splits at commas; an explicit pipe delimiter keeps embedded commas inside each list item.

The contract covers nonempty input and delimiter splitting. Value/display mappings and allowed-value validation are separate checks.

<a id="module.system-setting"></a>
[D · module.system-setting]

The legacy SystemSetting processor writes a named CSP setting during Compile, replacing its old value. Other phases, including Clean on uninstall, do not restore or delete that setting.

Only names whose first dot-separated segment is CSP are handled; the second segment is the setting key. The declared value is written to the current namespace's mapped `^%SYS("CSP", key)` global. New modules are directed by the product to use CPF instead. [Whether uninstall should retain or restore that setting is open](../../Verus_verification/open_questions.md#module.system-setting.cleanup-policy).

- Working assumption: Uninstall keeps the installed SystemSetting value.

<a id="module.webapp.package"></a>
[D · module.webapp.package]

Web-app resources are configuration and are marked handled during export, so their paths are not exported as database documents. Publishing preserves their declarations in the package.

After uninstall removes the app, installing the published package recreates it from those declarations. A `{$cspdir}` Path is evaluated against the installation's CSP root, so it survives the round trip and follows a different CSP root on another installation.

This guarantee assumes successful package/registry transport and app creation. It does not promise compensation for a partial provider failure.

<a id="module.python.explicit-wheel"></a>
[D · module.python.explicit-wheel]

An explicit PythonWheel resource installs its local wheel during Initialize, even with `-bypass-py-deps`; that flag skips requirements.txt processing instead. The wheel path comes from the module root, its declared Directory and its filename, and extra pip flags are passed through the shared pip runner after empty arguments are removed.

- A missing wheel or failed pip call fails an ordinary module. IPM's own module instead warns and continues with the resource unhandled, so setup can continue without that optional package.
- Clean marks the wheel handled and leaves the shared Python installation in place. Uninstalling a module therefore does not establish that its Python package is gone.
- [Cleanup of shared Python packages remains an owner decision](../../Verus_verification/open_questions.md#module.python.cleanup-policy). After a failed pip call, package presence is unconfirmed: pip can have made partial changes. Missing-file refusal never calls pip. Pip dependency resolution, online indexes and compensation after partial installation remain external boundaries.

- Working assumption: Clean preserves the current shared Python installation.

<a id="module.processor.file-copy.protection"></a>
[D · module.processor.file-copy.protection]

Directory protection is checked again at copy and Clean time. A marker at `target/IRIS.DAT` protects that directory even when it is not configured as a database. Otherwise the target must equal or contain a listed system, IPM or configured database directory, after path normalization and case folding. An ordinary child of a protected directory is not automatically protected.

- Adding a synthetic marker after load makes Clean keep the copied directory. Removing its marker before Clean makes the ordinary scratch directory eligible for removal. The checked commands succeed and remove the module record in both cases.
- A single-file resource inside a protected directory is still deleted on Clean when KeepOnUninstall is false; the surrounding directory and unrelated files remain.
- The marker check examines the target itself, not descendants. A scratch parent containing an otherwise unknown child with a synthetic IRIS.DAT is unprotected and Clean removes the whole parent. [The intended unknown-descendant policy remains open](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants).

- Working assumption: Protect configured roots and their ancestors and an exact target containing an IRIS.DAT marker.

<a id="module.processor.file-copy.unix-overlay"></a>
[D · module.processor.file-copy.unix-overlay]

On the pinned Unix runtime, an ordinary directory load with Overlay=0 copies the new payload but keeps unrelated old entries. The Unix CopyDir wrapper always enables the provider's overlay option; its delete-first argument instead controls deletion of matching files before replacement. Both direct flag values kept an unrelated sentinel while replacing an old same-name payload. This contradicts the documented full-replacement rule above and is C:D29.

Protected-directory checks still change the requested argument and Clean behavior. They must not be treated as proof that the Unix copy operation clears an unprotected target. The Windows wrapper explicitly removes the target first, but no Windows execution is claimed.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Only explicitly indexed clauses have iteration 3 formal contracts. Source links alone supply no proof.

### module

- human-provided
- user-facing-behavior
- Children: [identity](#module.identity), [resources](#module.resource), [dependencies](#module.dependency), [parameters](#module.parameter), [requirements](#module.requirement), [invokes](#module.invoke), [processors](#module.processor), [placement](#module.placement), [deployed code](#module.deployed), [Python](#module.python), [environment files](#module.env).

Sources: README L1-6; `wiki/03`; `wiki/00` L1-10.

### module.identity

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L99-115, L181-186; CHANGELOG 0.10.0 (`#755`, `#756`), 0.10.5 (`#959`, `#958`). Inventory: CAT-01. Test: `ExternalNameModuleAction.cls`. Observed: the manifest is validated on save, refusing an external-name clash or a resource owned by another module (`Storage/Module.cls:1510-1591`, OP-LOAD-09).

### module.resource

- human-provided
- user-facing-behavior
- Expands: [Module](#module).
- Children: [scope](#module.resource.scope), [stored identity](#module.resource.identity).

Sources: `wiki/03` L279-343; `wiki/00` L5-7. Inventory: CAT-02. `wiki/03` L302 misdescribes `Deploy` as not shipped. It means shipped as compiled code ([deployed code](#module.deployed)).

### module.resource.identity

- agent
- Expands: [Resources](#module.resource).

Sources: `Storage/Module.cls::%OnValidateObject`; `Storage/ResourceReference.cls` unique-name property/index. Verus `resources.rs::{resource_save_allowed,project_database,resource_identity_and_database_reader}`. Native reused-reference corrupt-install error leaves stored Deployed=true; a fresh duplicate reference instead fails validation with object=true and SQL row=false. These probes distinguish the two actual error origins. Other module-save guards remain separate obligations.

### module.resource.scope

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Resources](#module.resource).

Sources: CHANGELOG 0.10.11 (`#1023`), 0.10.7 (`#843`); `ResourceProcessor/Default/Document.cls:195-198`. Inventory: CAT-02, A1 seed 7; OP-LOAD-24. Tests: `Scopes.cls`, `IncludeTests.cls`. Open elsewhere: `Scope="verify"` is deferred (PR #1319).

### module.dependency

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L243-250; CHANGELOG 0.10.7 (`#408`), 0.10.6 (`#1072`). Inventory: CAT-03. Test: `DepNoVersion.cls`. Observed: a `Scope` on a dependency limits it to the test or verify phase, which only the code and tests describe ([phase scope](../features/dependencies.g5.md#deps)). `wiki/03` L247 has a wrong `^2.2.1` example.

### module.parameter

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

- Children: [expressions](#module.parameter.expressions), [literal values](#module.parameter.literal-values), [cycles](#module.parameter.cycles).

Sources: `wiki/03` L252-258; `wiki/00` L175-180; CHANGELOG 0.10.3 (`#779`), 0.10.7 (`#1013`). Inventory: CAT-04; OP-LOAD-05 (`Storage/Module.cls:799-831`). Native: a `-D` value overrides a default (E07), and chains resolve through several levels (E22, E31, E36).

Findings: [C:D3 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d3). The linked entry qualifies the caller, reader and providers.

Findings: [C:D4 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d4). The linked entry qualifies the caller, reader and providers.

Findings: [C:D5 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d5). The linked entry qualifies the caller, reader and providers.

### module.parameter.expressions

- uncertain
- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: doc comment `Utils/Module.cls:2678-2696` (system expressions and `$$$macros`); code `Storage/Module.cls:1833-1894` (order: macros, parameters, system values, `{name}`, `#{}`), `Storage/ModuleSetting/Default.cls:14-110`, `Utils/Module.cls:2697-2815`. Native corpus E01-E36 (`20261114T025636Z-r1-evaluator`, IRIS 2026.1 build 234U ARM64) and model `Verus_verification/consolidated/models/attr_evaluator.py`, which matches every observed value and rejects 23 wrong variants. Observed details a reader may need: `$$$OK no brace` stays unchanged (E15); a default named `version` does not override the module version (E20); a parameter name is used as a regular expression, so `a.b` also matches `{$axb}` (E21).

Extension Verus: [attr_evaluator.rs::evaluate](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::evaluate_expression](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::macro_helper](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::macro_value](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::parameters](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::regex_replace_match](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::system_expression](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [module.parameter.expressions.brace-policy](../../Verus_verification/open_questions.md#module.parameter.expressions.brace-policy)

### module.parameter.literal-values

- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: the product escapes backslashes in values for exactly this reason (`Utils/Module.cls:2811`, `:2813`). Native: a default `win=C:\dir\sub` is inserted unchanged (E34), but a default `price=$5` makes `{$price}` raise `<REGULAR EXPRESSION>` (E32), while `{$namespace}` in the same module still works (E33). Defect C:D4.

Extension Verus: [attr_evaluator.rs::regex_replace_match](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.parameter.cycles

- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: the product's own message "Circular reference or too many levels in placeholders" and its 20-level guard (`Storage/ModuleSetting/Default.cls:62-77`). The guard is unreachable: the loop condition already stops at zero. Native: `a=${b}`, `b=${a}` resolve without error to `${a}` (E30), and using `${a}` then raises `<REGULAR EXPRESSION>` (E18, E35) rather than the named error. Defect C:D3.

Extension Verus: [attr_evaluator.rs::resolve_bounded](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.requirement

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](#module).
- Children: [requirement values](#module.requirement.values).

Sources: `wiki/03` L260-272; CHANGELOG 0.9.0 (`#364`), 0.10.0 (`#469`), 0.10.4 (`#874`), 0.10.10 (`#106`). Inventory: CAT-05; OP-LOAD-12. Tests: `VersionReq.cls` (`IPMVersion`, `PythonVersion`), `SystemRequirements.cls`, `Health.cls`, `Interoperability.cls`. When the check runs is open ([timing](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)). `wiki/03` L267 dates `IPMVersion` to 0.9.0, CHANGELOG says 0.10.0.

### module.requirement.values

- agent
- Expands: [System requirements](#module.requirement).

Sources: `Storage/SystemRequirements.cls::{CheckRequirements,CheckHealth,CheckInteroperability,CheckSYSNamespace}`; tests `Health.cls`, `Interoperability.cls`, `SystemRequirements.cls`. Verus `resources.rs::{requirement_failures,requirement_neighbors}`. Native environment-subclass probes exercise the unmodified check bodies, not every product variant or version expression.

### module.invoke

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Iteration 3: [Invoke contracts](../../Verus_verification/consolidated/iter3/public_repairs.rs), `invoke_ok` and `invoke_selected`, with native checked/unchecked returned-error neighbors in [the index](../../uplift/consolidated/iter3/INDEX.json). Dictionary return type and argument evaluation are source bindings, not implementation proofs.

Sources: `wiki/03` L345-362; CHANGELOG 0.10.0 (`#530`), 0.10.5 (`#828`). Inventory: CAT-06. Tests: `Invoke.cls`, `CustomPhase.cls`. `wiki/03` L359 gives a stale `CheckStatus` default. Observed: an Invoke that returns an error leaves its phase's history entry open ([returned errors](../../Verus_verification/open_questions.md#history.phases.returned-errors)).

State machines: [public repairs · iter3](../../Verus_verification/models/iter3-public-repairs/manifest.json)

### module.processor

- human-provided
- user-facing-behavior
- Expands: [Module](#module).
- Children: [file copy](#module.processor.file-copy), [web applications](#module.webapp).

Sources: `wiki/03` L581-785; CHANGELOG 0.9.0 (`#562`, `#593`), 0.10.0 (`#702`), 0.10.11 (`#1274`, `#1232`). Inventory: CAT-07; OP-LOAD-24. Observed: the Compile phase ignores a processor's compile status (OBS-L8). `wiki/06` describes a `<WSGIApplication>` element with no processor.

Extension Verus: [thin_surfaces.rs::cpf_status](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.processor.defaults

- agent
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Abstract.cls::{%OnNew,SetParams,CopyAttributes,ApplyDefaults,CopyDefaultAttributes,SatisfiesCondition}`; `Storage/Module.cls::GetOrderedResourceList`. Verus `resources.rs::{defaults_step,apply_defaults,ordered_before,chained_defaults_and_last_writer,explicit_default_is_protected,resource_order}`. Defaults use literal equality conditions in the checked native/formal group; expression conversion, other condition operators and full list ordering are residuals.

### module.webapp

- uncertain
- agent
- user-facing-behavior
- Expands: [Processors](#module.processor).
- Children: [failure](#module.webapp.failure), [upgrade](#module.webapp.upgrade), [validation](#module.webapp.validation), [ownership question](../../Verus_verification/open_questions.md#module.webapp.ownership).

Direct decision: `ResourceProcessor/WebApplication.cls::{OnBeforePhase,CreateOrUpdateWebApp,DeleteWebApp,CopyAttributes,OnGetUniqueName}`. Readers and callers: `Storage/Module.cls::ExecutePhases` (processor `OnBeforePhase` for Configure, Activate, Unconfigure and Clean), `Storage/Module.cls` duplicate-resource check at manifest import (`:1568`), `%SYS` `Security.Applications::{Exists,Get,Modify,Create,Delete}`. CHANGELOG 0.9.0 `#562`. Verus [webapp.rs](../../Verus_verification/consolidated/iter5/webapp.rs) `{merged,install_app,modify_called,refused_at_import,lemma_adoption_keeps_undeclared,webapp_neighbors}`; controls `adoption-overwrites-undeclared`, `existing-app-not-adopted`, `modify-always-called` rejected. Native W1 (create), W2/W3 (administrator app adopted, then deleted), W4 (second module refused), W5 (manual edits) in a fresh container; replayed as Verus assertions. Iteration 5 discovery.

Open questions: [module.webapp.ownership](../../Verus_verification/open_questions.md#module.webapp.ownership)

State machines: [webapp · iter5](../../Verus_verification/models/iter5-webapp/manifest.json)

### module.webapp.failure

- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Direct decision: transaction scope of the load (`Utils/Module.cls` load path) over `%SYS` writes from `CreateOrUpdateWebApp`. Verus `failed_after_activate`; control `transaction-keeps-app` rejected. Native W8 (transaction: app and module gone) versus W8b (`-DNoTransaction=1`: both kept), using a fixture invoke that fails after Activate. Contrast: [CPF effects](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy) are not undone by rollback. Developer-mode commit points were not exercised.

### module.webapp.upgrade

- uncertain
- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Readers: `Storage/Module.cls::ExecutePhases` and `Lifecycle/Base.cls::%Clean` act only on the installed manifest's resources; no step compares the old and new manifests. Verus `upgrade_drops_declaration`; control `upgrade-removes-app` rejected. Native W10 (load 2.0.0 without the app, then uninstall: app still present). The `update` command from a registry was not run; this used `load` of a higher version.

Open questions: [module.webapp.upgrade.policy](../../Verus_verification/open_questions.md#module.webapp.upgrade.policy)

### module.webapp.validation

- agent
- user-facing-behavior
- Expands: [Web applications](#module.webapp).

Dead check: `ResourceProcessor/WebApplication.cls::OnPhase` (Validate branch, `DoPropertiesExist`). Missing caller: `Lifecycle/Base.cls::%Validate` returns OK without dispatching; the generic `ResourceProcessor/Abstract.cls::OnBeforePhase("Validate")` only runs `%ValidateObject`. CHANGELOG 0.10.11 `#806` quotes the refusal message as once user-visible. Verus `validate_dispatches`, `refused_at_validate`; control `validate-dispatches` rejected. Native W9 (`NoSuchProperty="1"`: load OK, app created).

### module.processor.file-copy

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Processors](#module.processor).

Sources: CHANGELOG 0.10.11 (`#300`, `#934`, `#1135`); `ResourceProcessor/FileCopy.cls:32-36`, `:222-240`. Inventory: CAT-07, A1 seed 11. Tests: `FileCopy.cls` (`KeepOnUninstall` on uninstall), `CopyDirOverlap.cls`. `KeepOnUninstall` is not yet in the wiki.

Extension Verus: [thin_surfaces.rs::clean_removes](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::copies_after](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::copies_before](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::file_copy_observation](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

The prior thin_surfaces `clears_target` is a requested-argument predicate, not a qualified Unix clearing guarantee. Its `copy_step` success clause removing unrelated entries loses Unix correspondence for unprotected Overlay=0 targets; earlier source/proofs/receipts are preserved. [Superseding boundary](#module.processor.file-copy.unix-overlay).

Open questions: [module.processor.file-copy.unknown-descendants](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants)

### module.placement

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L188-241, L274-277, L533-573; CHANGELOG 0.9.0 (`#575`), 0.10.4 (`#374`). Inventory: CAT-10. Tests: `Mappings.cls`, `ModuleRoot.cls`. `wiki/03` L775 documents `${authUnauthenticated}`, which is not implemented.

### module.deployed

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](#module).

Children: [project loading](#module.deployed.project-load). Iteration 3 Verus `resources.rs::deploy` and `deployment_precedence` derive class-parameter override and lifecycle defaults; actual native calls distinguish parameter false, resource true, Module and DeployedModule.

Iteration 4 producer repair: iteration 3 passed the lifecycle default in as a boolean, so swapping the two lifecycles' defaults could not break its proof. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{deploy_by_default,deploy,deploy_neighbors}` now takes the lifecycle class as input; control `lifecycle-defaults-swapped` is rejected, as are `explicit-false-ignored` and `explicit-overrides-class-parameter`. Producer: `Lifecycle/Base.cls` `DEPLOYBYDEFAULT=0` and the resolution at `:1945-1963`; `Lifecycle/DeployedModule.cls` `DEPLOYBYDEFAULT=1`. Native P-Base/P-Module 0 and P-DeployedModule 1 read the compiled parameters.

Sources: `wiki/05`; CHANGELOG 0.10.0 (`#754`), 0.10.4 (`#815`), 0.10.10 (`#1158`), 0.10.11 (`#1242`, `#1279`, `#1125`). Inventory: CAT-34. Tests: `DeployedItems.cls`, `DeployedPackageLoad.cls`, `OrasDeployedPlatformFilter.cls`. `wiki/05` L22-67 is stale about plain loads and removed phases.

State machines: [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### module.deployed.project-load

- agent
- Expands: [Deployment](#module.deployed).

Sources: `ResourceProcessor/Default/Document.cls::{OnPhase,LoadFromDeployedProject}`; `Lifecycle/Base.cls::%Export`; native `%Studio.Project.Check` contract and item counts. Verus `resources.rs::{load_project,check_project,load_project_boundaries,project_check_prunes_missing}`. Native receipts distinguish reused-reference installer error (object=true, SQL=true, marker absent) from fresh-duplicate save refusal (object=true, SQL=false, installer not reached). Success-marker installation is source/formal only. No proof of installer atomicity.

### module.python

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L117-178; `wiki/07`; README L52-58; `Main.cls:310-311`; CHANGELOG 0.9.0 (`#538`, `#595`, `#647`), 0.10.10. Inventory: CAT-08; OP-LOAD-25. Test: `ProcessPythonWheel.cls`. pip changes stay after a rollback ([rollback scope](transactions-and-dev-mode.g5.md#transaction.rollback-scope)).

Open questions: [module.python.cleanup-policy](../../Verus_verification/open_questions.md#module.python.cleanup-policy)

### module.env

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](#module).

- Children: [failure](../../Verus_verification/open_questions.md#module.env.failure).

Sources: `Main.cls:66`, `:73`, `:304`; CHANGELOG 0.10.0 (`#704`, `#743`), 0.10.10 (`#1260`). Inventory: CAT-37. Test: `EnvironmentConfig.cls`. The help points to a wiki page that does not exist. Code: `General/EnvironmentConfig.cls:9-129`, `Lifecycle/Base.cls:2188-2200` (clear, then load in order; called in every Reload phase, `:631`, and by the phase commands, `Main.cls:2755`, `:2832`), `Utils/LegacyCompat.cls:36-41` (single-process compile). Consolidated round 5 native runs `20261114T033831Z-r5-env` and `20261114T033908Z-r5-env-b` (ARM64) and model `Verus_verification/consolidated/models/env_config.py`: deep merge and position-by-position arrays (U2, U2r); a missing, malformed or array file fails and empties the configuration (U3-U5); a later scalar replaces an object, a later object onto a scalar fails (U6, U6r); `${CONS_VAR}` fills from the OS environment, an unset one is empty (U7); `a$1b` becomes `aCONS_DOLLARb`, `a\b` becomes `ab`, `a$9b` raises `<REGULAR EXPRESSION>` (U7, U8; defect C:D14); in one interactive session the next commands read an empty configuration (S1); a compile-time generator sees the configuration only in a single-process compile (M1-M4), which IPM selects while a configuration is loaded (S3).

Extension Verus: [env_config.rs::clear](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::command](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::evaluate_json](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::generator_visible](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::is_empty](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::load_file](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::lookup](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::merge_value](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::singleton_get](../../Verus_verification/consolidated/verus/env_config.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D14 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d14). The linked entry qualifies the caller, reader and providers.

Open questions: [module.env.failure](../../Verus_verification/open_questions.md#module.env.failure)

### module.processor.cpf-effects

- uncertain
- agent
- user-facing-behavior
- Expands: [Processors](#module.processor).
- Children: [compensation policy](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy).

Sources: `ResourceProcessor/CPF.cls::GetSourceFile`, `::DoMerge`, `::MergeCPF`; [exact bindings, premises and final readers](../../uplift/consolidated/iter2/boundaries/bindings.json); [native and Verus receipts](../../uplift/consolidated/iter2/boundaries/REPORT.md). Hand translation under explicit provider outcomes; no ObjectScript-body proof.

Open questions: [module.processor.cpf-effects.policy](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy)

### module.global.roundtrip

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Modules](#module).

Sources: `ResourceProcessor/Default/Global.cls::{OnItemRelativePath,OnResolveChildren,OnPhase}` → `Lifecycle/Base.cls::ExportSingleModule` and `$system.OBJ.Load`. Native S6-02 explicitly kills `^My.Settings` before archive load and reads Parameter=42, so a surviving value cannot supply the result. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `global_path`, `package_global`, `reload_global`. Serialization and database provider success remain premises. [Index](../../uplift/consolidated/iter6/INDEX.md).

State machines: [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)

### module.processor.default-sweep

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.processor](#module.processor).

Sources and readers: `Storage/ModuleSetting/Default.cls::EvaluateAttribute` → `ResourceProcessor/Abstract.cls::%Evaluate`; `Utils/Module.cls::%RegExReplace`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `replace`, `sweep`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.csp.directory

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.processor](#module.processor).

Sources and readers: `ResourceProcessor/CSPApplication.cls::OnBeforePhase` → `OnPhaseInternal`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `csp_directory`, `csp_after_before`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.webapp.provider-status

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.webapp](#module.webapp).

Sources and readers: `ResourceProcessor/WebApplication.cls::CreateOrUpdateWebApp` → `OnBeforePhase` → `Storage/Module.cls::ExecutePhases`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `differs`, `app_status`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.list.delimiter

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.resource](#module.resource).

Sources and readers: `DataType/ListOfOptions.cls::XSDToLogical` → generated property conversions and `ResourceProcessor/Abstract.cls::CopyAttributes`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `split`, `delimiter`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.system-setting

- uncertain
- agent; user-facing-behavior; native-tested; Verus-scoped
- Expands: [Processors](#module.processor).

Sources: `ResourceProcessor/SystemSetting.cls::{OnPhase,OnResolveChildren}` → `Storage/Module.cls::ExecutePhases`. Product test `SystemSetting::TestSystemSetting` supplies the independent seed. Native S7-10 first sets the value to old, loads the product fixture, reads 123, uninstalls and reads 123 again. [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `system_target`, `system_value`. Global mapping and database transaction semantics are provider premises. [Index](../../uplift/consolidated/iter7/INDEX.md).

Open questions: [module.system-setting.cleanup-policy](../../Verus_verification/open_questions.md#module.system-setting.cleanup-policy)

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.webapp.package

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [module.webapp](#module.webapp).

Sources and readers: `ResourceProcessor/WebApplication.cls::OnExportItem` → `Lifecycle/Base.cls::{ExportSingleModule,%Publish}`, `Lifecycle/Module.cls::%Package`, registry payload/manifest readers → `WebApplication.cls::CreateOrUpdateWebApp`; final reader `Security.Applications.Get`. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/webapps.rs): `package_app`, `evaluate_path`, `uninstall`, `reinstall`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [webapps · iter8](../../Verus_verification/models/iter8-webapps/manifest.json)

### module.python.explicit-wheel

- uncertain
- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [module.python](#module.python).

Sources and readers: `ResourceProcessor/PythonWheel.cls::{OnPhase,OnResolveChildren}` → `Lifecycle/Base.cls::InstallOrDownloadPythonRequirements`, `Utils/Module.cls::PipInstall` and subsequent Python metadata/import readers. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/python.rs): `wheel_path`, `requirements_called`, `wheel`, `without_empty`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

Open questions: [module.python.cleanup-policy](../../Verus_verification/open_questions.md#module.python.cleanup-policy)

State machines: [python · iter8](../../Verus_verification/models/iter8-python/manifest.json)

### module.processor.file-copy.protection

- uncertain
- agent; source-inspected; native-tested; Verus-scoped
- Expands: [FileCopy](#module.processor.file-copy).

[Classifier and lifecycle model](../../Verus_verification/consolidated/iter14/protection.rs): `prefix`, `ancestor_of_root`, `protected`, `marker_write`, `clean`, `lifecycle_neighbors`. Source: Utils.File::{IsProtectedDirectory,DatabaseDirectories,NormalizeForComparison}; FileCopy::{NormalizeNames,DoCopy,OnPhase}. Paths are already normalized lowercase segments; root enumeration and raw path/file classification remain provider boundaries. Native scratch load/uninstall checks files, marker changes and module rows separately. No real database directory was modified.

Open questions: [module.processor.file-copy.unknown-descendants](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants)

State machines: [protection · iter14](../../Verus_verification/models/iter14-protection/manifest.json)

### module.processor.file-copy.unix-overlay

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [FileCopy](#module.processor.file-copy).

[Unix provider model](../../Verus_verification/consolidated/iter14/protection.rs): `delete_first`, `unix_args`, `unix_copy`, `copy`. Source: FileCopy Overlay property and DoCopy; Utils.File::CopyDir Unix and contrasting Windows branches. [Direct calibration and provider reference](../../uplift/consolidated/iter14/provider-reference.md). The initial expected-clear model and failed native replay remain visible; bytes/status/rows are native qualification, not implementation proofs.

Findings: [C:D29 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d29). The linked entry qualifies the caller, reader and providers.

State machines: [protection · iter14](../../Verus_verification/models/iter14-protection/manifest.json)
