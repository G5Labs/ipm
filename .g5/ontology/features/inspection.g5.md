# Inspecting modules and the environment

Commands that show information without installing or removing anything. For the log of past changes, see [history](history.g5.md#history.read).

## Statements

<a id="inspect"></a>
[I · inspect]

These commands show information without changing installed modules:

- `search` lists modules in all registries, filtered to this platform. `-versions` shows every version.
- `list-installed` lists installed modules, optionally as a tree, with Python packages or sorted.
- `list-dependents` lists modules that depend on a given module.
- `show` shows one installed module's details, resources, dependencies and the modules that require it. It looks up stored `Name` first, then `ExternalName`; if both could match, `Name` wins. Neither match produces an error naming the missing module and namespace.
- Name and alias lookup use the module datatype's case-insensitive normalization. Native stored-name, alias, uppercase-alias and missing-name neighbors preserve the selected record and version.
- `info` and `version` show the IPM environment and version.
- `namespace` (`zn`) lists modules by namespace and switches to a namespace.
- `orphans` lists items in the namespace's code database that belong to no module.
- `help` shows help for the shell or for one command.

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

### inspect.webapp.detail

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [inspect](#inspect).

Sources and readers: `General/ModuleSummary.cls::GetWebApplicationDetail` → summary rendering; `GetAttribute` supplies first nonempty values. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `detail`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.
