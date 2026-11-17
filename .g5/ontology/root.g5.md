# InterSystems Package Manager

<a id="ipm"></a>
[I · ipm]

IPM installs, updates and removes modules in an IRIS namespace, together with their dependencies. It gets them from local folders, archives, git repositories or package registries.

## From source to namespace

IPM records package-change attempts in the namespace where they run.

```
local folder, archive or Git checkout  -> load    ┐
configured repository                  -> install ├-> manifest -> dependencies -> lifecycle phases
installed module + selected source     -> update  ┘
installed module                       -> uninstall -> Clean
```

## How the pages are organized

A page’s home follows the question its statements answer. The folder does not change a statement’s ID, its meaning, or whether it records intent or a decision.

| Category | When a rule belongs here | IPM example |
|---|---|---|
| Concept | Several workflows depend on the same meaning, policy or constraint. Define vocabulary in the page introduction and give the shared rule one owner. | Which source a finalized installation record identifies. [Data policy](concepts/data-policy.g5.md#history.fields.source-snapshot) |
| Architecture | The decision concerns how IPM components carry out a shared responsibility, including the order of steps or the component responsible for an effect. | How the phase runner includes prerequisites for load, test and other commands. [Lifecycle execution](architecture/lifecycle.g5.md#lifecycle.chain) |
| Operation | An administrator installs IPM itself, inspects or changes its configuration, or manages where it is available. | How an operator enables IPM in another namespace and chooses whether to share its repository settings. [Namespace administration](operations/namespaces.g5.md#namespace.enable) |
| Feature | The task concerns a package and the outcome a caller can observe when authoring, installing, testing, publishing, inspecting or removing it. | Which version and supplier a reinstall uses. [Reinstall](features/install.g5.md#reinstall.selection) |

For example, adding a repository is an operation. The rule for choosing among enabled repositories is shared by several commands and belongs under concepts. How a filesystem provider builds its catalog belongs under architecture. What happens when a caller asks to reinstall a module belongs to the feature. Each page can link to the others where their rules meet.

## Shared rules

These pages own rules used by several commands.

- [Data policy](concepts/data-policy.g5.md) governs module identity, namespace boundaries, package origin and the lifetime of history records.
- [Transaction and developer-mode rules](concepts/transactions-and-dev-mode.g5.md) determine which database writes can survive failure.
- [Download credentials and integrity](concepts/download-security.g5.md) covers registry redirects, archive checks and Artifactory credentials.
- [Test selection and results](concepts/testing-policy.g5.md) covers which tests count and how failure reaches a CI job.
- [Version compatibility](concepts/versions.g5.md) and [package source selection](concepts/repositories.g5.md) govern the versions and suppliers eligible for a request.

## Package workflows

- Install or change packages — [Load](features/load.g5.md) · [Install or reinstall](features/install.g5.md) · [Update](features/update.g5.md) · [Uninstall](features/uninstall.g5.md) · [Resolve dependencies](features/dependencies.g5.md)
- Inspect a namespace — [Inspect installed modules](features/inspection.g5.md) · [Read and manage history](features/history.g5.md)
- Develop a module — [Write the manifest](features/manifest.g5.md) · [Develop and sync](features/developer-tools.g5.md) · [Install resources](features/resources.g5.md) · [Configure web applications](features/web-applications.g5.md) · [Download build artifacts](features/artifact-download.g5.md)
- Ship a release — [Test, package, verify and publish](features/build-and-publish.g5.md) · [Ship compiled code](features/packaging-deployed-code.g5.md)

## Operating IPM

[Install or upgrade IPM](operations/ipm-setup.g5.md) · [Enable or unmap namespaces](operations/namespaces.g5.md) · [Manage repositories](operations/repositories.g5.md) · [Configure settings and defaults](operations/settings.g5.md)

## How package operations work

[Lifecycle execution](architecture/lifecycle.g5.md) explains how module commands select phases and run callbacks. [Repository providers](architecture/package-sources.g5.md) explains how filesystem, remote and OCI sources deliver candidate packages.

## Running a command from a script

<a id="ipm.shell"></a>
[I · ipm.shell]

Users run IPM commands in the `zpm` shell or as `zpm "<command>"`. Programs call `%IPM.Main:Shell()`, which returns a status that shows whether the command succeeded.

<a id="ipm.shell.forms"></a>
[D · ipm.shell.forms]

Only `%IPM.Main:Shell()` lets a script see that a command failed. `Shell(command)` returns the status, and `Shell(command, 1)` also ends the process with exit code 1 on failure.

- The `zpm "<command>"` command form discards the status. Adding `,1` does not pass a flag; the process keeps running and exits 0.
- As built, the `$zpm("<command>")` function form does not return the status. It runs the command, then fails with `<COMMAND> Function must return a value` whether the command passed or failed (C:D24).

## Metadata

The uplift describes IPM at `571e04a1` (`v0.10.11-beta.28`). Recovered intent is provisional until accepted by its owners. A model link identifies the checked cases and assumptions. Some runs document a mismatch between the intended rule and the pinned product. The [Verus foundation](../Verus_verification/README.md), [consolidated assessment](../uplift/consolidated/CONVERGENCE-20261116.md) and [open questions](../Verus_verification/open_questions.md) give the evidence and its limits.

<a id="how-to-read-this-ontology"></a>
**Sources and notation.** I marks intended behavior and D marks a design decision. Q identifies an unresolved question in the registers. Metadata names the source and links to applicable findings, questions and model records. A working assumption is provisional.

Unqualified source paths such as `Main.cls` or `General/History.cls` are under `src/cls/IPM/` at the pinned revision. Paths beginning with `preload/` or `tests/` and root documents are repository-relative. Integration test classes are under `tests/integration_tests/Test/PM/Integration/` and unit test classes under `tests/unit_tests/Test/PM/Unit/`. Wiki page numbers use [the project wiki](https://github.com/intersystems/ipm/wiki) at `781e98fd`. CAT, INT and Q references use the [intent inventory](../uplift/ipm-verus-20261113/A/intent-inventory.md). OP, OBS and P use the [operation inventory](../uplift/ipm-verus-20261113/A/operation-inventory.md). The [coverage map](../uplift/ipm-verus-20261113/A/coverage-map.md) joins them.

Earlier uplift evidence: [iteration 3](../uplift/consolidated/iter3/INDEX.json), [iteration 4](../uplift/consolidated/iter4/INDEX.json), [iteration 5](../uplift/consolidated/iter5/INDEX.json), [iteration 6](../uplift/consolidated/iter6/INDEX.md), [iteration 7](../uplift/consolidated/iter7/INDEX.md), [iteration 8](../uplift/consolidated/iter8/INDEX.md), [iteration 9](../uplift/consolidated/iter9/INDEX.md), [iteration 10](../uplift/consolidated/iter10/INDEX.md).

### ipm

- human-provided
- user-facing-behavior
- Children: [shell](#ipm.shell).

Sources: README L1-6; vendor manual AIPM. Inventory: A1 section 1.

### ipm.shell

- human-provided
- user-facing-behavior
- Expands: [IPM](#ipm).

Sources: `wiki/01` L109-167; CHANGELOG 0.10.9 (`#1186`); CONTRIBUTING L115-156. Inventory: CAT-41; A2 1.1 (`%IPM.Main:Shell`, `ShellInternal`), 1.3 (`ShellScript`, `zpm` command routine). Observed: `Shell` runs the daily [history purge](concepts/data-policy.g5.md#history.retention) before each command and returns its error without running the command if it fails. The `zpm "<command>"` form discards the status. The function form was meant to return it, but see [entry forms](#ipm.shell.forms): as built it raises an error (corrected in iteration 5).
- Children: [entry forms](#ipm.shell.forms).

### ipm.shell.forms

- design-verified

- agent
- user-facing-behavior
- Expands: [Shell](#ipm.shell).

Direct decision: `Main.cls::Shell` (`pTerminateOnError`), `Main.cls::LanguageExtensions` (the `ZPM` body, lowercase `do ##class(%IPM.Main).Shell(pArgs...) quit` since commit `8eb520fd`, 2025-07-02), `Main.cls::UpdateLanguageExtensions` (builds `%ZLANGF00` by rewriting only lines containing the exact text `Do ##class`, unchanged by that commit). Before `8eb520fd` the same rewrite produced `Quit ##class(...)`. Verus [entry_forms.rs](../Verus_verification/consolidated/iter5/entry_forms.rs) `{function_returns_value,pinned_body,outcome,entry_neighbors}`; controls `matcher-case-insensitive`, `pinned-body-capitalized`, `command-flag-terminates`, `plain-shell-terminates` rejected. Native Z1-Z5 through stdin sessions on a fresh container, including the installed `%ZLANGF00` text. Why the command form ignores `,1` was not traced; only its outcome is claimed.

State machines: [entry forms · iter5](../Verus_verification/models/iter5-entry-forms/manifest.json)
