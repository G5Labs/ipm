# InterSystems Package Manager (IPM)

IPM installs, updates and removes modules in InterSystems IRIS. A module is a package of ObjectScript code, data, files, web applications and Python dependencies, described by a `module.xml` manifest. IPM gets modules from local folders, archives, git repositories or package registries, installs their dependencies, and keeps a log of every change it makes in each namespace.

This ontology states what IPM is meant to do. It was recovered from the README, CHANGELOG, wiki, command help, issues, pull requests and tests of IPM at commit `571e04a1` (tag `v0.10.11-beta.28`). Loading packages and the transaction history log are covered in depth, because they are the scope of the formal verification. Other capabilities are covered at the level a reader needs to find and rebuild them.

## How the parts fit

```
  user or script
      │  zpm "<command>"   or   %IPM.Main:Shell()
      ▼
  IPM shell ─── settings (config) and default modifiers
      │
      ├─ install  ─► find module in repositories ─┐
      ├─ update   ─► find newer version ──────────┤
      ├─ load     ─► local folder, archive, git ──┤
      │                                           ▼
      │                         shared loading path
      │                         checks ─► transaction ─► manifest ─► dependencies
      │                                           │
      │                                           ▼
      │                         lifecycle phases: Initialize → Reload →
      │                         Validate → Compile → Activate (→ ApplyUpdateSteps)
      │
      ├─ uninstall ─► Clean phase
      ├─ test, package, verify, publish ─► lifecycle phases
      │
      └─ every install, load, uninstall and update
               └─► history log of the namespace (survives rollback)
```

A package change runs in one IRIS transaction, so a failure is rolled back. Developer mode (`-dev`) and the `-DNoTransaction` and `-DNoJournal` flags change what a failure keeps, and the history log's `Committed` column reports it.

## Pages

Shared concepts:

- [Modules and the manifest](concepts/modules.g5.md) (`module.*`): what a `module.xml` can declare, such as resources, dependencies, parameters, system requirements, invokes, processors, deployed code and Python packages.
- [Lifecycle phases](concepts/lifecycle.g5.md) (`lifecycle.*`): the fixed phase order, phase chains, `-only`, `module-action` and custom phases.
- [Transactions and developer mode](concepts/transactions-and-dev-mode.g5.md) (`transaction.*`, `dev-mode.*`): how a failed change is rolled back, the flags that turn that off, and what developer mode changes.
- [Namespaces](concepts/namespaces.g5.md) (`namespace.*`): per-namespace installation, `enable`, `unmap` and legacy mapping.
- [Repositories](concepts/repositories.g5.md) (`repo.*`): remote, ORAS and filesystem repositories, search order and read-only repositories.
- [Versions](concepts/versions.g5.md) (`version.*`): semantic versions, ranges, snapshots and prereleases.
- [Settings](concepts/settings.g5.md) (`settings.*`): `config` keys, default modifiers and installation flags.

Features:

- [Loading a package](features/load.g5.md) (`load.*`), in depth: paths, the shared loading path, refusals, what a failed load keeps, developer-mode commits and dependency loads.
- [Transaction history](features/history.g5.md) (`history.*`), in depth: what is recorded, when, the `Committed` and `Success` fields, reading, deleting, retention and protection of records.
- [Installing and reinstalling](features/install.g5.md) (`install.*`, `reinstall.*`): getting modules from repositories.
- [Updating](features/update.g5.md) (`update.*`): updating to a newer version and running update steps.
- [Uninstalling](features/uninstall.g5.md) (`uninstall.*`): removing modules, dependents, `-recurse`, `-all` and removing IPM itself.
- [Dependency resolution and lock files](features/dependencies.g5.md) (`deps.*`): picking dependency versions, conflicts and `module-lock.json`.
- [Testing, packaging and publishing](features/build-and-publish.g5.md) (`build.*`): `test`, `package`, `verify`, `publish`, `unpublish` and `makedeployed`.
- [Developer tools](features/developer-tools.g5.md) (`devtools.*`): `sync`, `generate`, `init`, `arrange`, `module-version` and scripting commands.
- [Inspecting modules](features/inspection.g5.md) (`inspect`): `search`, `list-installed`, `show`, `info` and similar read-only commands.
- [Installing and upgrading IPM](features/ipm-setup.g5.md) (`setup.*`): the installer and upgrades from legacy versions.

## How to read this ontology

- `[I]` blocks state intended behavior, `[D]` blocks state important decisions, and `[Q]` blocks hold open questions with a provisional answer.
- Each page ends with metadata for every block: its source, tags, links to parent and child blocks, and notes on observed behavior or suspected gaps. Observed behavior is not a requirement.
- Source locators use these short forms. A code path with a line, such as `Main.cls:268` or `General/History.cls:103`, is under `src/cls/IPM/` at commit `571e04a1`. Paths starting with `preload/` or `tests/`, the root `module.xml`, README, CHANGELOG and CONTRIBUTING are from the repository root. `wiki/NN` is page NN of the [project wiki](https://github.com/intersystems/ipm/wiki) at revision `781e98fd`.
- A test is named as a class and method, such as `History.cls` `TestDevMode`. Test classes are under `tests/integration_tests/Test/PM/Integration/`, and `Unit/CLI.cls` is under `tests/unit_tests/Test/PM/`.
- `CAT`, `INT` and `Q` IDs point to the [intent inventory](../uplift/ipm-verus-20261113/A/intent-inventory.md). `OP`, `OBS` and `P` IDs point to the [operation inventory](../uplift/ipm-verus-20261113/A/operation-inventory.md). The [coverage map](../uplift/ipm-verus-20261113/A/coverage-map.md) joins them.

## System-wide statements

<a id="ipm"></a>
[I · ipm]

IPM installs, updates and removes modules in an IRIS namespace, together with their dependencies. It gets them from local folders, archives, git repositories or package registries.

<a id="ipm.shell"></a>
[I · ipm.shell]

Users run IPM commands in the `zpm` shell or as `zpm "<command>"`. Programs call `%IPM.Main:Shell()`, which returns a status that shows whether the command succeeded.

## Metadata

### ipm

- human-provided
- user-facing-behavior
- Children: [shell](#ipm.shell).

Sources: README L1-6; vendor manual AIPM. Inventory: A1 section 1.

### ipm.shell

- human-provided
- user-facing-behavior
- Expands: [IPM](#ipm).

Sources: `wiki/01` L109-167; CHANGELOG 0.10.9 (`#1186`); CONTRIBUTING L115-156. Inventory: CAT-41; A2 1.1 (`%IPM.Main:Shell`, `ShellInternal`), 1.3 (`ShellScript`, `zpm` command routine). Observed: `Shell` runs the daily [history purge](features/history.g5.md#history.retention) before each command and returns its error without running the command if it fails. The `zpm "<command>"` form discards the status, while the `$$ZPM` function form returns it.
