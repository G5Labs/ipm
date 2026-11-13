# Developer tools

Commands that help people write and maintain modules in a namespace. None of them is recorded in [history](history.g5.md#history.actions).

## Definitions

- **File hash**: a fingerprint of a file's contents that `sync` uses to tell whether the file changed.

## Statements

<a id="devtools"></a>
[I · devtools]

IPM includes tools to keep developer-mode modules in step with files on disk, to write manifests and to script IPM.

<a id="devtools.sync"></a>
[I · devtools.sync]

`sync [<module>]` brings changed files from disk into developer-mode modules without a full reload. It finds files changed since the last load or sync by their hash and recompiles only what is out of date.

- With no module, it syncs every developer-mode module in dependency order.
- `-delete` also removes items whose files were deleted, and `-test` runs test classes that changed.
- `load -skip-sync` leaves recording the hashes to the next `sync`.

<a id="devtools.sync.retry"></a>
[I · devtools.sync.retry]

If a file fails to sync, the next `sync` tries it again.

<a id="devtools.authoring"></a>
[I · devtools.authoring]

Commands for writing modules:

- `generate` writes a `module.xml`, asking for details, including a tests folder.
- `init` sets up a namespace for IPM and its editor extensions.
- `arrange` puts a manifest's resources in the standard order.
- `dependency-analyzer` finds references to items outside the module and its dependencies.
- `module-version` sets or bumps a module's version, and refuses a downgrade unless `-force` is given.

<a id="devtools.scripting"></a>
[I · devtools.scripting]

Commands for scripting:

- `run-from-file` runs IPM commands listed in a `.txt` file or a JSON file.
- `exec` runs ObjectScript.
- `import` loads classes from files and re-exports them to source control if needed.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). These blocks are outside the formal scope.

### devtools

- human-provided
- user-facing-behavior
- Children: [sync](#devtools.sync), [authoring](#devtools.authoring), [scripting](#devtools.scripting).

Sources: `Main.cls:85-119`, `:322-404`, `:509-522`, `:639-705`, `:781-805`. Inventory: CAT-31, CAT-36.

### devtools.sync

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer tools](#devtools).

Sources: `Main.cls:108-119`; `wiki/02` L1136-1189; CHANGELOG 0.10.10 (`#1117`), 0.10.11 (`#1290`). Inventory: CAT-31; A2 1.2 row 6 (`Main:Sync`, `General/Sync/Pipeline.cls`). Tests: `Sync.cls` `TestNoChangeIsNoOp`, `TestModifiedClassRecompiles`, `TestLoadSkipSyncStampsNoBaseline`. Observed: `sync` runs without a transaction.

### devtools.sync.retry

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Sync](#devtools.sync).

Sources: issue #1224; `General/Sync/Pipeline.cls:146-153`, `:201-222`. Inventory: CAT-31, A1 seed 10. Test: `Sync.cls` L653-734.

### devtools.authoring

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Developer tools](#devtools).

Sources: `Main.cls:85-99`, `:639-705`, `:781-805`; CHANGELOG 0.10.11 (`#150`, `#806`, `#503`), 0.10.0 (`#710`). Inventory: CAT-36; A2 1.2 rows 4, 29, 30, 32, 37. Tests: `Generate.cls`, `ModuleVersion.cls`.

### devtools.scripting

- human-provided
- user-facing-behavior
- Expands: [Developer tools](#devtools).

Sources: `Main.cls:322-404`, `:509-522`; CHANGELOG 0.10.6 (`#996`). Inventory: CAT-36; A2 1.2 rows 18, 19, 23. The `run-from-file` help says the JSON root is "a single array" (`Main.cls:352`), while the examples and code need an object with `commands`. Observed: each line of `run-from-file` runs as its own shell command with its own history record.
