# Inspecting modules and the environment

Commands that show information without installing or removing anything. For the log of past changes, see [history](history.g5.md#history.read).

## Statements

<a id="inspect"></a>
[I · inspect]

These commands show information without changing installed modules:

- `search` lists modules in all registries, filtered to this platform. `-versions` shows every version.
- `list-installed` lists installed modules, optionally as a tree, with Python packages or sorted.
- `list-dependents` lists modules that depend on a given module.
- `information` prints instance details and the installed top-level modules: every installed module, IPM included, that no installed module names as a dependency. It shows the external name (with `-v`, `External (name)`) and the version without build metadata.
- `show` shows one installed module's details, resources, dependencies and the modules that require it. It looks up stored `Name` first, then `ExternalName`; if both could match, `Name` wins. Neither match produces an error naming the missing module and namespace.
- Name and alias lookup use the module datatype's case-insensitive normalization. Native stored-name, alias, uppercase-alias and missing-name neighbors preserve the selected record and version.
- `info` and `version` show the IPM environment and version.
- `namespace` (`zn`) lists modules by namespace and switches to a namespace.
- `orphans` lists items in the namespace's code database that belong to no module.
- `help` shows help for the shell or for one command ([help details](#inspect.help)).

<a id="inspect.help"></a>
[D · inspect.help]

`help` lists every command with its aliases and summary. `help <command>` or an alias shows that command's arguments, flags and examples, and phase commands such as `compile` also show `module-action`'s flags.

- An unknown name, including a different-case name such as `INSTALL`, prints a warning and the full list.
- `version` prints IPM's version per namespace and each remote registry's version. The shell intro names the current registry only when a remote registry is defined.

<a id="inspect.show"></a>
[D · inspect.show]

The `show` summary prints only non-empty blocks: description and fields, the after-install message and `<Default>` values, resources by kind, dependencies with their install status, direct dependents, and system requirements.

- Scoped and unit-test resources, `<Defaults><Parameter>` values and keywords are not shown, and a FileCopy target is shown as written, before expansion.
- As built, a `Health="false"` system requirement is not shown, although it stops installation on IRIS for Health (defect C:D52).

<a id="inspect.orphans.ownership"></a>
[D · inspect.orphans.ownership]

`orphans -type CLS` subtracts classes for which the home-module lookup returns an installed module. A class can disappear from this list while its code stays callable.

- Loading an unrelated module that explicitly declares a controlled class creates its resource ownership and removes that class from orphan output. Another unclaimed class remains visible.
- A module name matching a class/package prefix can also supply its home, even if that module declares no resources. This naming convention can hide a class without an explicit resource reference.
- Explicit resource ownership is checked before the module-name fallback. A later normal explicit claim can change the selected home from the naming module to the declaring module, while orphan visibility stays hidden.
- These observations cover ordinary compiled, nonstub classes in the current namespace's default code database. Other resource types, mapped/system/generated items, arbitrary namespaces, inventory failures and overlapping query lifetimes remain outside this new qualification. Untyped inventory noise remains inherited P:F-UNIN-7.

<a id="inspect.orphans.identifier-case"></a>
[D · inspect.orphans.identifier-case]

The checked catalog and CLI return class names ending in lowercase `.cls`. The home-module API tests uppercase `CLS` literally before taking its class-specific ancestor-package path; it does not first normalize that extension.

- In the checked direct naming-only lookup, lowercase input returns the full `.cls` resource name, while uppercase input returns the matching `.PKG` name. Both select the same naming module and no resource-reference object.
- Explicit claims in the selected cases resolve in both forms through the stored-name lookup. Generic SQL collation and package/stub behavior across other spellings are not established by these examples.

<a id="inspect.dependents"></a>
[D · inspect.dependents]

As built, `show` lists only direct, unscoped dependents and hides scoped dependencies from its dependency list. This display is not the complete dependency graph used for removal decisions.

- A missing installed dependency version reads `not installed`; a present version that fails the requested range reads `unsatisfied`.
- Requirements-file display skips blank lines and lines starting with `#` after trimming. Inline comments remain in the displayed line.

<a id="inspect.list-installed"></a>
[D · inspect.list-installed]

`list-installed` prints installed modules with their versions: IPM first, then the others by displayed name. A module with an external name shows as `External (name)`.

- A developer-mode module is marked `(DeveloperMode)`, and `-showsource` adds each module's root.
- `-s` sorts by name, version or date, ascending or descending, and IPM stays first. As built, `-s version` compares prereleases of the same version as text, so `1.0.0-rc.10` sorts before `1.0.0-rc.2` (defect C:D42).
- `-tree` ignores `-s`. It starts from installed modules that no other installed module depends on, in name order, and prints each dependency under every module that needs it, marking an uninstalled one `[missing]`.

<a id="inspect.namespace"></a>
[D · inspect.namespace]

`zn <name>` switches the session to that namespace only when IPM is available there. Otherwise it prints "IPM is not enabled" and leaves the namespace and the command status unchanged, while an unknown namespace fails the command.

- `zn <pattern>` lists the matching namespaces, marking those without IPM, and switches to the one the user enters. An empty answer keeps the current namespace.

<a id="inspect.list-dependents"></a>
[D · inspect.list-dependents]

`list-dependents <module> [<version>]` lists every installed module whose dependency graph contains the module, directly or through other modules. A version argument keeps only dependents whose graph resolves the module to that version.

- With `-r <repos>`, it lists repository manifests instead and ignores installed modules.
- A dependent whose graph cannot be resolved is printed as a warning instead of a dependent. When the module is absent, the namespace's installed requirements on it can make every dependent's graph fail this way.
- As built, `-t` prints only the module and the modules between it and the top of each graph. It never prints a module that is itself the top of a dependency graph, so those dependents are missing from the tree (defect C:D38).

<a id="inspect.webapp.detail"></a>
[D · inspect.webapp.detail]

The web-app summary shows its DispatchClass first, then its WSGIAppLocation, then its Path or Directory. Empty attributes are skipped, and it shows no detail if all four are empty.

The text identifies the selected value as `dispatch`, `wsgi` or `files at`. It reads the declared resource attributes, not a fresh query of effective Security.Applications properties.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). The inherited scope excluded this page; iteration 2 adds only the explicitly linked scoped reader contracts.

### inspect

- human-provided
- user-facing-behavior
- test-coverage
- Children: [CLS orphan ownership](#inspect.orphans.ownership), [identifier case](#inspect.orphans.identifier-case).

Sources: `Main.cls:17-27`, `:524-616`, `:683-724`, `:807-816`; CHANGELOG 0.10.11 (`#1019`), 0.10.7 (`#1079`), 0.10.5 (`#950`), 0.10.10 (`#1158`). Inventory: CAT-17, CAT-25; A2 1.2 rows 1, 24-27, 31, 33, 34, 38. Tests: `Show.cls`, `Unit/CLI.cls` `TestSortList`, `ListInstPythonLibrariesTest.cls`. No wiki page covers `show` or `info`.

Extension Verus: [thin_surfaces.rs::inspection_choice](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.


Iteration51 `information`: [toplevel.rs](../../Verus_verification/consolidated/iter51/toplevel.rs) `listed`, `neighbors` ([handoff](../../uplift/consolidated/iter51/HANDOFF.md)).
### inspect.help

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Sources: `CLI.cls` (`%Help`, `%HelpForCommand*`, `UnifyLifecycles`, `%GetCommandStructure`); `Main.cls::Version`, `::GetVersion`, `::BuildIntroMessages`. Model: [helpcmd.rs](../../Verus_verification/consolidated/iter54/helpcmd.rs) `help`, `shows_only`, `intro_current_registry`, `neighbors` ([handoff](../../uplift/consolidated/iter54/HANDOFF.md)). The remote-registry part of `version` is outside the run's scope.

### inspect.show

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Sources: `General/ModuleSummary.cls` (`GetSystemRequirements`, `AddList`, `AddTable`, `AddBlock`, `GetSection`, `SplitLines`). Iteration46 summary: [summary.rs](../../Verus_verification/consolidated/iter46/summary.rs) `health_false_hidden`, `parameter_not_listed`, `empty_blocks_omitted`, `scoped_hidden` ([handoff](../../uplift/consolidated/iter46/HANDOFF.md)).

Findings: [`show` omits a `Health="false"` system requirement](../../uplift/consolidated/BUGS.md#c-d52)

### inspect.orphans.ownership

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Inspection](#inspect).

[Ownership and orphan-reader contract](../../Verus_verification/consolidated/iter20/ownership.rs) separates physical code, stored modules, resource relationships, home lookup and visible rows. [Iteration20](../../uplift/consolidated/iter20/HANDOFF.md) records normal compile/load producers, actual inventory, stored owner/child data, home-name API and CLI readers. A reference exists only as data; an actual module object is required for ownership. Provider/persistence/inventory boundaries and the two-class projection are explicit.

### inspect.orphans.identifier-case

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [CLS orphan ownership](#inspect.orphans.ownership).

[Case-preserving lookup](../../Verus_verification/consolidated/iter20/ownership.rs) keeps actual lowercase catalog input separate from uppercase direct calls. The first native gate's uppercase observer was wrong and is retained as a producer-format miss; the corrected observations do not establish a new product defect. [Evidence and limits](../../uplift/consolidated/iter20/HANDOFF.md).

### inspect.dependents

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Authority: product method comments and `General/ModuleSummary.cls::GetDirectDependents`, `::GetLines`, `::GetInstalledStatus`, `::ParseRequirements`. [Scoped Verus reader contracts](../../Verus_verification/consolidated/iter2/readers.rs) and [native reader observations](../../uplift/consolidated/iter2/native/receipt.json) distinguish selection and output; SQL ordering, arbitrary formatting and the whole graph remain outside these proofs.

Findings: [P:F-DEP-3 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-dep-3). The linked entry qualifies the caller, reader and providers.

### inspect.list-installed

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Sources: `Main.cls::ListInstalled`, `::SortList`, `::DisplayModules`, `::GetListModule`, `::AccumulateTreeRecursive`. Model: [listing.rs](../../Verus_verification/consolidated/iter35/listing.rs) `flat_pos`, `sort_neighbors`, `tree`, `tree_neighbors` ([handoff](../../uplift/consolidated/iter35/HANDOFF.md)). Date sort, `-python`, `-showupstream`, `-description`, `-repository` and namespace patterns are not assessed.

Findings: [`list-installed -s version` orders prereleases of one version as text](../../uplift/consolidated/BUGS.md#c-d42)

### inspect.namespace

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Sources: `Main.cls::Namespace`, `::IsIPMEnabled`. Model: [nsinit.rs](../../Verus_verification/consolidated/iter55/nsinit.rs) `zn`, `zn_neighbors` ([handoff](../../uplift/consolidated/iter55/HANDOFF.md)). Switching into a second IPM namespace is not assessed.

### inspect.list-dependents

- agent
- user-facing-behavior
- Expands: [Inspection](#inspect).

Sources: `Main.cls::ListDependents`, `::PrintTree`; `Utils/Module.cls::GetDependentsList`, `::GetDependentsAsTree`, `::BuildAllDependencyGraphs`, `::InvertDependencies`. Model: [dependents.rs](../../Verus_verification/consolidated/iter33/dependents.rs) `listed`, `warned`, `flat_neighbors`, `repo_neighbors`, `removed_target_neighbors`, `tree_omits_roots`. Iteration33 native cases replay the flat, version-filtered, repository and tree reports ([handoff](../../uplift/consolidated/iter33/HANDOFF.md)).

Findings: [`list-dependents -t` leaves out modules at the top of a dependency graph](../../uplift/consolidated/BUGS.md#c-d38)

### inspect.webapp.detail

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [inspect](#inspect).

Sources and readers: `General/ModuleSummary.cls::GetWebApplicationDetail` → summary rendering; `GetAttribute` supplies first nonempty values. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `detail`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.
