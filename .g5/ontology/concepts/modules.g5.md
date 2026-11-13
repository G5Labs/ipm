# Modules and the manifest

A module is what IPM installs. This page defines what a module's manifest can declare. How IPM acts on those declarations is owned by the [lifecycle](lifecycle.g5.md), [loading](../features/load.g5.md) and the other feature pages.

## Definitions

- **Manifest**: the `module.xml` file that describes a module.
- **Resource**: one item a module installs, such as a class, routine, include file, global, file or web application.
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

<a id="module.requirement"></a>
[I · module.requirement]

`<SystemRequirements>` lists conditions that must hold before the module is installed. A failed condition [refuses the load](../features/load.g5.md#load.refusal.requirements).

- `Version`: the IRIS versions the module supports.
- `Interoperability`: the namespace must have interoperability enabled.
- `Health`: the instance must be HealthShare, HealthConnect or IRIS for Health.
- `IPMVersion` and `PythonVersion`: the IPM and Python versions it needs.
- `SYSNamespace`: the module must be installed in `%SYS`.

<a id="module.invoke"></a>
[I · module.invoke]

`<Invoke>` runs a class method during a lifecycle phase, by default after the Configure phase. If the method returns a status, IPM checks it, and an error fails the phase.

- `CustomPhase` makes the method run only when that [custom phase](lifecycle.g5.md#lifecycle.custom) is requested.

<a id="module.processor"></a>
[I · module.processor]

Resource processors with special behavior include:

- `FileCopy`, which copies files or folders ([file copy](#module.processor.file-copy)).
- `WebApplication`, which sets up web applications. `CSPApplication` is deprecated.
- `CPF`, which merges configuration into the instance.
- `SystemSetting`, `ArtifactoryTarball`, unit test and localization processors.

<a id="module.processor.file-copy"></a>
[I · module.processor.file-copy]

`FileCopy` copies files at install and removes them at uninstall.

- It never deletes IRIS or IPM system folders, or any folder that contains one.
- It refuses to copy when the source and target are the same folder or one contains the other.
- `KeepOnUninstall` leaves the copied files in place on `uninstall` and `clean`, for files other modules may share.

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

A resource with `Deploy="true"` ships as compiled code without its source. IPM picks the package built for the current IRIS version and platform, and installs deployed code into the database each resource maps to.

<a id="module.python"></a>
[I · module.python]

A module can depend on Python packages.

- IPM installs `requirements.txt` with pip in the Reload phase, and `<PythonWheel>` files in Initialize.
- `-bypass-py-deps` skips this, and `-extra-pip-flags` adds pip flags.
- Python 3.9 or later is required. The `PipCaller` and `UseStandalonePip` settings pick which pip runs.

<a id="module.env"></a>
[I · module.env]

`-env` takes `;`-separated JSON files of settings for the module, merges them and replaces `${VAR}` with environment values. The module reads them with `%IPM.General.EnvironmentConfig:GetArg`, and a missing file is an error.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). These blocks are outside the formal scope.

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
- Children: [scope](#module.resource.scope).

Sources: `wiki/03` L279-343; `wiki/00` L5-7. Inventory: CAT-02. `wiki/03` L302 misdescribes `Deploy` as not shipped. It means shipped as compiled code ([deployed code](#module.deployed)).

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

Sources: `wiki/03` L252-258; `wiki/00` L175-180; CHANGELOG 0.10.3 (`#779`), 0.10.7 (`#1013`). Inventory: CAT-04; OP-LOAD-05 (`Storage/Module.cls:799-831`).

### module.requirement

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](#module).

Sources: `wiki/03` L260-272; CHANGELOG 0.9.0 (`#364`), 0.10.0 (`#469`), 0.10.4 (`#874`), 0.10.10 (`#106`). Inventory: CAT-05; OP-LOAD-12. Tests: `VersionReq.cls` (`IPMVersion`, `PythonVersion`), `SystemRequirements.cls`, `Health.cls`, `Interoperability.cls`. When the check runs is open ([timing](../features/load.g5.md#load.refusal.requirements.timing)). `wiki/03` L267 dates `IPMVersion` to 0.9.0, CHANGELOG says 0.10.0.

### module.invoke

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L345-362; CHANGELOG 0.10.0 (`#530`), 0.10.5 (`#828`). Inventory: CAT-06. Tests: `Invoke.cls`, `CustomPhase.cls`. `wiki/03` L359 gives a stale `CheckStatus` default. Observed: an Invoke that returns an error leaves its phase's history entry open ([returned errors](../features/history.g5.md#history.phases.returned-errors)).

### module.processor

- human-provided
- user-facing-behavior
- Expands: [Module](#module).
- Children: [file copy](#module.processor.file-copy).

Sources: `wiki/03` L581-785; CHANGELOG 0.9.0 (`#562`, `#593`), 0.10.0 (`#702`), 0.10.11 (`#1274`, `#1232`). Inventory: CAT-07; OP-LOAD-24. Observed: the Compile phase ignores a processor's compile status (OBS-L8). `wiki/06` describes a `<WSGIApplication>` element with no processor.

### module.processor.file-copy

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Processors](#module.processor).

Sources: CHANGELOG 0.10.11 (`#300`, `#934`, `#1135`); `ResourceProcessor/FileCopy.cls:32-36`, `:222-240`. Inventory: CAT-07, A1 seed 11. Tests: `FileCopy.cls` (`KeepOnUninstall` on uninstall), `CopyDirOverlap.cls`. `KeepOnUninstall` is not yet in the wiki.

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

Sources: `wiki/05`; CHANGELOG 0.10.0 (`#754`), 0.10.4 (`#815`), 0.10.10 (`#1158`), 0.10.11 (`#1242`, `#1279`, `#1125`). Inventory: CAT-34. Tests: `DeployedItems.cls`, `DeployedPackageLoad.cls`, `OrasDeployedPlatformFilter.cls`. `wiki/05` L22-67 is stale about plain loads and removed phases.

### module.python

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L117-178; `wiki/07`; README L52-58; `Main.cls:310-311`; CHANGELOG 0.9.0 (`#538`, `#595`, `#647`), 0.10.10. Inventory: CAT-08; OP-LOAD-25. Test: `ProcessPythonWheel.cls`. pip changes stay after a rollback ([rollback scope](transactions-and-dev-mode.g5.md#transaction.rollback-scope)).

### module.env

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](#module).

Sources: `Main.cls:66`, `:73`, `:304`; CHANGELOG 0.10.0 (`#704`, `#743`), 0.10.10 (`#1260`). Inventory: CAT-37. Test: `EnvironmentConfig.cls`. The help points to a wiki page that does not exist.
