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

<a id="reinstall.selection"></a>
[D · reinstall.selection]

`reinstall` reads the installed version and searches the enabled repositories again for that version. It does not preserve the stored supplier or use the stored module root as a fallback.

- In the checked plain `1.0.0` case, a module installed from beta reloads alpha's files when alpha is the older eligible repository. Alpha also offers `2.0.0`, but reinstall still requests and loads `1.0.0`. Disabling alpha makes the same reinstall reload beta instead.
- With both repository definitions removed, ordinary and `-dev` reinstalls fail even though the installed code and its source folder remain. The saved supplier, root and callable payload stay beta's.
- Current `show` and repository-filtered lists follow the newly selected supplier. Earlier successful history retains its past supplier, even after that definition is deleted. These are the shared [source-reader rules](../concepts/repositories.g5.md#repo.origin.installed) applied to this caller.
- The qualification covers stable filesystem sources, no dependencies and successful ordinary nondeveloper loads. Snapshot dependency refresh, successful developer-mode reinstall, other versions and general provider failures remain outside it. The [current-versus-latest question](#reinstall.version) remains open.

<a id="reinstall.history"></a>
[D · reinstall.history]

A reinstall of an existing module delegates to `install` and creates one install-history record in the checked no-dependency cases. A no-repository failure still creates a failed record, while a missing installed-module name is rejected before delegation and creates no record.

Successful records keep the original `reinstall` command and selected source. The checked failure rows appear with their error in `history find`; their other fields and detailed status rendering are not newly qualified here.

<a id="reinstall.snapshot-dependencies"></a>
[D · reinstall.snapshot-dependencies]

Default reinstall can refuse to refresh an installed snapshot dependency because its exact requirement conflicts with the added installed-version lower bound. This violates the [snapshot refresh requirement](#reinstall); the underlying [range-intersection defect](../concepts/versions.g5.md#version.range.snapshot-intersection) is C:D33.

- In the checked filesystem case, changing dependency `1.0.0+snapshot` from payload 1 to 2 leaves the installed payload at 1. `-shallow` succeeds and keeps 1; default reinstall reports the range conflict and also keeps 1.
- Supplying `-DPermitDowngrade=1` skips that added lower bound, selects the dependency from alpha and reloads it. Both a direct dependency call and a call through its parent then return 2. A release dependency `1.0.0` keeps payload 1 under default reinstall.
- This qualifies one exact dependency per parent, successful directory loads and the selected conflict refusal. Other ranges, graph shapes, providers, failures and concurrent source changes remain outside it. No product repair is applied or certified.

<a id="reinstall.version"></a>
[Q · reinstall.version]

Does `reinstall` install the version already installed, or the latest available version?

- Provisional answer: the version already installed, as the command summary and its examples say.
- Alternative: the latest version, as the command description and the wiki say.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). The inherited formal scope covered the shared loading path. Later scoped models add the explicitly linked repository and caller relations; they do not verify generic transport or complete command bodies.

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
- Children: [selection](#reinstall.selection), [history](#reinstall.history), [snapshot dependencies](#reinstall.snapshot-dependencies), [version](#reinstall.version).

Sources: `Main.cls:447-481`; `wiki/02` L879-911. Inventory: CAT-14; OP-LOAD-22 (`Main:Reinstall`). Observed: recorded in history as `install`. A `reinstall` of a module that is not installed is refused before any history record is created.

### reinstall.selection

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Reinstall](#reinstall).

[Caller and selection contract](../../Verus_verification/consolidated/iter22/reinstall.rs) derives the requested version from the installed record, searches the eligible catalog again, then reuses the [same-version effect](../concepts/repositories.g5.md#repo.origin.same-version). [Iteration 22](../../uplift/consolidated/iter22/HANDOFF.md) records the available newer neighbor, creation-order tie, disabled and deleted definitions, current readers, successful source snapshots and explicit provider limits. Snapshot defaults are represented only as a source-derived configuration projection, without new dependency-outcome qualification.

[Iteration 25 source-file qualification](../concepts/transactions-and-dev-mode.g5.md#dev-mode.source-files) extends the caller evidence to successful developer-mode reinstalls and the following normal-mode refusal in two directory layouts. It does not settle the current-versus-latest question or the developer-mode force/load policy.

### reinstall.history

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Reinstall](#reinstall).

[History effect contract](../../Verus_verification/consolidated/iter22/reinstall.rs) distinguishes the early no-delegation branch from install initialization before repository refusal. It reuses successful history records and separately counts delegated install records without inventing failed-row fields. [Receipts](../../uplift/consolidated/iter22/HANDOFF.md) include persistent-row counts, summary readers, original command production and retained source snapshots. The shared [history action rules](history.g5.md#history.actions) and unresolved refusal policy retain their existing owners.

### reinstall.snapshot-dependencies

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Reinstall](#reinstall).

[Dependency composition](../../Verus_verification/consolidated/iter24/snapshot_dependencies.rs) connects the caller's default or shallow flag, parsed snapshot classification, installed reuse, inherited version-expression intersection, the graph's empty-server marker and dependency loading. [Iteration 24](../../uplift/consolidated/iter24/HANDOFF.md) preserves the initial refresh prediction and its refutation, the explicit permission neighbor, graph and flat-reference readers, physical source edits and callable parent results. This extends iteration 22's configuration-only snapshot scope. The checked error and retained payload do not prove general rollback or history behavior.

### reinstall.version

- agent
- uncertain
- user-facing-behavior
- Expands: [Reinstall](#reinstall).

Sources: A1 conflict 27; summary `Main.cls:448`; examples `Main.cls:455-458`; description `Main.cls:450-451`; `wiki/02` L880; `wiki/01` L40-44. Observed: the code pins the installed version (`Main.cls:2695-2697`). [Iteration 22](../../uplift/consolidated/iter22/HANDOFF.md) also observes a `1.0.0` reinstall while `2.0.0` is available. This establishes the checked as-built choice, not an owner decision between the conflicting descriptions.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#reinstall.version). The recorded assumption is provisional; no new maintainer approval was found.
