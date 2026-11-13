# Installing and reinstalling

`install` gets a module from a repository. It uses the [shared loading path](load.g5.md#load.core), so the load refusals, outcomes and history rules apply to it as well.

## Definitions

- **Enabled repository**: a [repository](../concepts/repositories.g5.md#repo) set up in the namespace and not disabled.
- **Package**: the archive a registry stores for one version of a module.

## Statements

<a id="install"></a>
[I · install]

`install <module> [<version>]` finds a module in the enabled repositories and installs it, with its dependencies, into the current namespace.

- `<repo>/<module>` picks one repository.
- The version can be exact or a [range](../concepts/versions.g5.md#version.range). Without one, IPM installs the latest available version.
- `-latest` installs the latest without asking. `-prompt` asks which version to install when several are found. `-keywords` searches by keywords.
- `-dev`, `-force` and the other load options work [as for `load`](load.g5.md#load.options).

<a id="install.refusal"></a>
[I · install.refusal]

IPM refuses an install when:

- No repository is enabled in the namespace.
- The module, or the requested version, is not found in any repository.
- The module is IPM itself and IPM is [mapped from another namespace](../concepts/namespaces.g5.md#namespace.mapped-refusal).
- A [load refusal](load.g5.md#load.refusal) or the [developer-mode overwrite guard](../concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard) applies.

<a id="install.package"></a>
[I · install.package]

IPM downloads the package, unpacks it into `<ModuleRoot>/<name>/<version>/` and loads the module from there. For modules with deployed code, IPM picks the package built for this IRIS version and platform.

<a id="reinstall"></a>
[I · reinstall]

`reinstall <module>` installs an installed module again from a repository. By default it also refreshes dependencies that have `+snapshot` versions, and `-shallow` turns that off.

<a id="reinstall.version"></a>
[Q · reinstall.version]

Does `reinstall` install the version already installed, or the latest available version?

- Provisional answer: the version already installed, as the command summary and its examples say.
- Alternative: the latest version, as the command description and the wiki say.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). `install` is in the formal scope where it uses the shared loading path. Repository search and download are outside it.

### install

- human-provided
- user-facing-behavior
- test-coverage
- Children: [refusals](#install.refusal), [package](#install.package).

Sources: `Main.cls:406-445`; `wiki/02` L409-472; README L62. Inventory: CAT-13, INT-LOAD-04; OP-LOAD-17 (`Main:Install`), OP-LOAD-18 (`Utils.Module:LoadQualifiedReference`, `LoadModuleReference`). Tests: `History.cls` `TestSimpleHistory`; `InstallModule.cls` `TestSimpleApp`. Observed: an empty answer to the `-prompt` choice installs nothing and reports success.

### install.refusal

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Install](#install).

Sources: messages `Main.cls:2573-2575`, `:2581-2586`, `:2654-2659`, `:2534-2552`. Inventory: CAT-13; OP-LOAD-17, OP-LOAD-18. Tests: `InstallModule.cls` `TestInstallForceFlag`, `TestDevMode`. Observed: a repository that cannot be reached is refused with "Repository '%1' is unavailable".

### install.package

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Install](#install).

Sources: `wiki/02` L651 ("matching the behavior of `install`"); CHANGELOG 0.10.10 (`#1158`), 0.10.7 (`#1053`). Inventory: CAT-13, CAT-34; OP-LOAD-18, OP-LOAD-19 (`Utils.Module:LoadModuleFromArchive`). Tests: `ModuleRoot.cls`, `OrasDeployedPlatformFilter.cls`. Observed: the download is checked against its SHA-1 hash, and the module's existing folder is replaced before any version check ([rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope)). A filesystem repository installs from the repository's own folder.

### reinstall

- human-provided
- user-facing-behavior
- Children: [version](#reinstall.version).

Sources: `Main.cls:447-481`; `wiki/02` L879-911. Inventory: CAT-14; OP-LOAD-22 (`Main:Reinstall`). Observed: recorded in history as `install`. A `reinstall` of a module that is not installed is refused before any history record is created.

### reinstall.version

- agent
- uncertain
- user-facing-behavior
- Expands: [Reinstall](#reinstall).

Sources: A1 conflict 27; summary `Main.cls:448`; examples `Main.cls:455-458`; description `Main.cls:450-451`; `wiki/02` L880; `wiki/01` L40-44. Observed: the code pins the installed version (`Main.cls:2695-2697`).
