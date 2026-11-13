# Inspecting modules and the environment

Commands that show information without installing or removing anything. For the log of past changes, see [history](history.g5.md#history.read).

## Statements

<a id="inspect"></a>
[I · inspect]

These commands show information without changing installed modules:

- `search` lists modules in all registries, filtered to this platform. `-versions` shows every version.
- `list-installed` lists installed modules, optionally as a tree, with Python packages or sorted.
- `list-dependents` lists modules that depend on a given module.
- `show` shows one installed module's details, resources, dependencies and the modules that require it.
- `info` and `version` show the IPM environment and version.
- `namespace` (`zn`) lists modules by namespace and switches to a namespace.
- `orphans` lists items in the namespace's code database that belong to no module.
- `help` shows help for the shell or for one command.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). This block is outside the formal scope.

### inspect

- human-provided
- user-facing-behavior
- test-coverage

Sources: `Main.cls:17-27`, `:524-616`, `:683-724`, `:807-816`; CHANGELOG 0.10.11 (`#1019`), 0.10.7 (`#1079`), 0.10.5 (`#950`), 0.10.10 (`#1158`). Inventory: CAT-17, CAT-25; A2 1.2 rows 1, 24-27, 31, 33, 34, 38. Tests: `Show.cls`, `Unit/CLI.cls` `TestSortList`, `ListInstPythonLibrariesTest.cls`. No wiki page covers `show` or `info`.
