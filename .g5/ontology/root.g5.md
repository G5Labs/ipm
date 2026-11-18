# InterSystems Package Manager

<a id="ipm"></a>

IPM manages packages and their dependencies in an InterSystems IRIS namespace. Start with [package commands](features/package-commands.g5.md#ipm) for the supported actions and package sources.

## How the pages are organized

Choose a page by the question you want answered. A single subject, such as a repository, can appear in all four categories.

| Category | What belongs here | Example for a repository |
|---|---|---|
| Concept | A shared term, policy or constraint that several workflows rely on. | Which supplier wins when several enabled repositories offer the same eligible version? [Package source selection](concepts/repositories.g5.md#repo.search-order) |
| Architecture | How IPM components carry out a responsibility, including who does the work and in what order. | How does the filesystem provider build and refresh its catalog? [Repository providers](architecture/package-sources.g5.md#repo.filesystem) |
| Operation | How an administrator installs, configures or inspects IPM itself and the environment where it runs. | How do I add, enable or disable a repository in a namespace? [Managing repositories](operations/repositories.g5.md#repo.manage) |
| Feature | What a caller can do with a package and what outcome the caller can observe. | Which supplier does IPM use when I reinstall a package? [Reinstall selection](features/install.g5.md#reinstall.selection) |

## From source to namespace

IPM records package-change attempts in the namespace where they run.

```
local folder, archive or Git checkout  -> load    ┐
configured repository                  -> install ├-> manifest -> dependencies -> lifecycle phases
installed module + selected source     -> update  ┘
installed module                       -> uninstall -> Clean
```

## Shared rules

These pages own rules used by several commands.

- [Data policy](concepts/data-policy.g5.md) governs module identity, namespace boundaries, package origin and the lifetime of history records.
- [Transaction and developer-mode rules](concepts/transactions-and-dev-mode.g5.md) determine which database writes can survive failure.
- [Download credentials and integrity](concepts/download-security.g5.md) covers registry redirects, archive checks and Artifactory credentials.
- [Test selection and results](concepts/testing-policy.g5.md) covers which tests count and how failure reaches a CI job.
- [Version compatibility](concepts/versions.g5.md) and [package source selection](concepts/repositories.g5.md) govern the versions and suppliers eligible for a request.

## Package workflows

<a id="ipm.shell"></a>
<a id="ipm.shell.forms"></a>

Run IPM from the [interactive or scripted shell](features/package-commands.g5.md#ipm.shell). For CI, see [which invocation returns a status or sets the process exit code](features/package-commands.g5.md#ipm.shell.forms).

- Install or change packages — [Load](features/load.g5.md) · [Install or reinstall](features/install.g5.md) · [Update](features/update.g5.md) · [Uninstall](features/uninstall.g5.md) · [Resolve dependencies](features/dependencies.g5.md)
- Inspect a namespace — [Inspect installed modules](features/inspection.g5.md) · [Read and manage history](features/history.g5.md)
- Develop a module — [Write the manifest](features/manifest.g5.md) · [Develop and sync](features/developer-tools.g5.md) · [Install resources](features/resources.g5.md) · [Configure web applications](features/web-applications.g5.md) · [Download build artifacts](features/artifact-download.g5.md)
- Ship a release — [Test, package, verify and publish](features/build-and-publish.g5.md) · [Ship compiled code](features/packaging-deployed-code.g5.md)

## Operating IPM

[Install or upgrade IPM](operations/ipm-setup.g5.md) · [Enable or unmap namespaces](operations/namespaces.g5.md) · [Manage repositories](operations/repositories.g5.md) · [Configure settings and defaults](operations/settings.g5.md)

## How package operations work

[Lifecycle execution](architecture/lifecycle.g5.md) explains how module commands select phases and run callbacks. [Repository providers](architecture/package-sources.g5.md) explains how filesystem, remote and OCI sources deliver candidate packages.

## Sources and verification

The uplift describes IPM at `571e04a1` (`v0.10.11-beta.28`). Recovered intent is provisional until accepted by its owners. A model link identifies the checked cases and assumptions. Some runs document a mismatch between the intended rule and the pinned product. The [Verus foundation](../Verus_verification/README.md), [consolidated assessment](../uplift/consolidated/CONVERGENCE-20261116.md) and [open questions](../Verus_verification/open_questions.md) give the evidence and its limits.

<a id="how-to-read-this-ontology"></a>
**Sources and notation.** I marks intended behavior and D marks a design decision. Q identifies an unresolved question in the registers. Metadata names the source and links to applicable findings, questions and model records. A working assumption is provisional.

Unqualified source paths such as `Main.cls` or `General/History.cls` are under `src/cls/IPM/` at the pinned revision. Paths beginning with `preload/` or `tests/` and root documents are repository-relative. Integration test classes are under `tests/integration_tests/Test/PM/Integration/` and unit test classes under `tests/unit_tests/Test/PM/Unit/`. Wiki page numbers use [the project wiki](https://github.com/intersystems/ipm/wiki) at `781e98fd`. CAT, INT and Q references use the [intent inventory](../uplift/ipm-verus-20261113/A/intent-inventory.md). OP, OBS and P use the [operation inventory](../uplift/ipm-verus-20261113/A/operation-inventory.md). The [coverage map](../uplift/ipm-verus-20261113/A/coverage-map.md) joins them.

Earlier uplift evidence: [iteration 3](../uplift/consolidated/iter3/INDEX.json), [iteration 4](../uplift/consolidated/iter4/INDEX.json), [iteration 5](../uplift/consolidated/iter5/INDEX.json), [iteration 6](../uplift/consolidated/iter6/INDEX.md), [iteration 7](../uplift/consolidated/iter7/INDEX.md), [iteration 8](../uplift/consolidated/iter8/INDEX.md), [iteration 9](../uplift/consolidated/iter9/INDEX.md), [iteration 10](../uplift/consolidated/iter10/INDEX.md).
